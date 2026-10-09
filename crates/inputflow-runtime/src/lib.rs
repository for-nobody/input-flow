//! Reusable InputFlow lifecycle owner shared by the diagnostic probe and the
//! product agent.
//!
//! The runtime owns configuration, the Hook/message-loop thread, bounded
//! diagnostic delivery, capture sessions, and clean shutdown. Platform control
//! requests remain serialized on the Hook owner by `inputflow-windows`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use inputflow_config::{
    Config, SCHEMA_VERSION, SaveReport, load, save_with_report, validate_config,
};
use inputflow_engine::{Key, Matcher, RuleIndex, SystemClock};
use inputflow_windows::platform::windows;

pub use inputflow_windows::platform::windows::{
    CaptureOutcome, CaptureSession, CapturedInput, ControlFailure, ControlFailureKind, PauseReport,
    Percentiles, ReplaceReport,
};

const LOG_QUEUE_CAPACITY: usize = 1024;
const DEFAULT_PENDING_CAPACITY: usize = 16;

pub type DiagnosticSink = Arc<dyn Fn(String) + Send + Sync + 'static>;

pub struct RuntimeOptions {
    pub config_path: PathBuf,
    pub crash_marker_path: PathBuf,
    pub debug_input: bool,
    pub pending_capacity: usize,
    pub diagnostic_sink: DiagnosticSink,
}

impl RuntimeOptions {
    pub fn new(diagnostic_sink: DiagnosticSink) -> Self {
        let data_dir = default_data_dir();
        Self {
            config_path: data_dir.join("config.json"),
            crash_marker_path: data_dir.join("running"),
            debug_input: false,
            pending_capacity: DEFAULT_PENDING_CAPACITY,
            diagnostic_sink,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePhase {
    Ready,
    ShuttingDown,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub phase: RuntimePhase,
    pub hook_thread_id: u32,
    pub suspended: bool,
    pub capture_active: bool,
    pub rule_count: usize,
    pub observed_events: u64,
    pub emergency_key: Key,
    pub config_path: PathBuf,
    pub output_batches_sent: u64,
    pub output_batches_failed: u64,
    pub output_batches_dropped: u64,
    pub callback_latency_us: Option<Percentiles>,
    pub hold_delay_us: Option<Percentiles>,
    pub state_revision: u64,
    pub apply_reconciliation: ApplyReconciliation,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartReport {
    pub hook_thread_id: u32,
    pub rule_count: usize,
    pub emergency_key: Key,
    pub source_schema_version: u32,
    pub config_write_blocked: bool,
    pub config_problems: Vec<String>,
    pub previous_abnormal_termination: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    ValidationFailed,
    PersistenceFailed,
    RuntimeCancelled,
    RuntimeFailed,
    RuntimeOutcomeUnknown,
    RuntimeBusy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyReconciliation {
    Settled,
    Pending { request_id: u64 },
    AppliedAfterTimeout { request_id: u64 },
    RolledBackAfterTimeout { request_id: u64 },
    RecoveryRequired { request_id: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyReport {
    pub outcome: ApplyOutcome,
    pub rule_count: usize,
    pub validation_errors: Vec<String>,
    pub save: Option<SaveReport>,
    pub runtime: Option<ReplaceReport>,
    pub rollback: Option<SaveReport>,
    pub runtime_request_id: Option<u64>,
    pub error: Option<String>,
    pub recovery_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport {
    pub quit_posted: bool,
    pub hook_thread_panicked: bool,
    pub logger_thread_panicked: bool,
    pub observed_events: u64,
    pub output_batches_sent: u64,
    pub output_batches_failed: u64,
    pub output_batches_dropped: u64,
}

struct RuntimeMetadata {
    phase: RuntimePhase,
    emergency_key: Key,
    apply_reconciliation: ApplyReconciliation,
    last_error: Option<String>,
}

struct RuntimeThreads {
    hook: JoinHandle<()>,
    logger: JoinHandle<()>,
}

pub struct Runtime {
    config_path: PathBuf,
    config_write_block_reason: Option<String>,
    current_config: Arc<Mutex<Config>>,
    apply_lock: Mutex<()>,
    metadata: Arc<Mutex<RuntimeMetadata>>,
    hook_thread_id: AtomicU32,
    rule_count: Arc<AtomicUsize>,
    logger_shutdown: Arc<AtomicBool>,
    threads: Mutex<Option<RuntimeThreads>>,
    reconciliation_thread: Mutex<Option<JoinHandle<()>>>,
    marker: CrashMarker,
}

impl Runtime {
    pub fn start(options: RuntimeOptions) -> Result<(Self, StartReport), String> {
        if options.pending_capacity == 0 {
            return Err("pending capacity must be greater than zero".to_string());
        }

        let marker = CrashMarker::new(options.crash_marker_path);
        let previous_abnormal_termination = marker.was_left_running();
        marker.mark_running()?;

        let prepared = (|| {
            let loaded = load(&options.config_path);
            let index = RuleIndex::compile(loaded.rules.clone()).map_err(|errors| {
                format!(
                    "validated configuration failed defensive rule compilation: {}",
                    errors
                        .into_iter()
                        .map(|error| error.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            })?;
            let matcher = Matcher::new(
                Box::new(SystemClock::new()),
                index,
                options.pending_capacity,
            );

            windows::reset_runtime_state()?;
            windows::set_debug_log(options.debug_input);
            windows::install_matcher(matcher)?;
            windows::install_emergency_key(loaded.emergency_key)?;

            let (log_tx, log_rx) = mpsc::sync_channel::<String>(LOG_QUEUE_CAPACITY);
            windows::install_log_sender(log_tx)?;
            Ok::<_, String>((loaded, log_rx))
        })();
        let (loaded, log_rx) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                windows::clear_log_sender();
                marker.clear();
                return Err(error);
            }
        };
        let logger_shutdown = Arc::new(AtomicBool::new(false));
        let logger_stop = Arc::clone(&logger_shutdown);
        let sink = Arc::clone(&options.diagnostic_sink);
        let logger = std::thread::spawn(move || logger_loop(log_rx, logger_stop, sink));

        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let hook = std::thread::spawn(move || windows::run_hook_thread(ready_tx));
        let hook_thread_id = match ready_rx.recv() {
            Ok(Ok(thread_id)) => thread_id,
            Ok(Err(error)) => {
                logger_shutdown.store(true, Ordering::Release);
                windows::clear_log_sender();
                let _ = hook.join();
                let _ = logger.join();
                marker.clear();
                return Err(error);
            }
            Err(_) => {
                logger_shutdown.store(true, Ordering::Release);
                windows::clear_log_sender();
                let _ = hook.join();
                let _ = logger.join();
                marker.clear();
                return Err("hook thread ended before reporting readiness".to_string());
            }
        };

        let rule_count = loaded.rules.len();
        let emergency_key = loaded.emergency_key;
        let source_schema_version = loaded.source_schema_version;
        let config_write_block_reason = loaded.write_blocked_reason.clone();
        let config_write_blocked = config_write_block_reason.is_some();
        let config_problems = loaded.problems.clone();
        let runtime = Self {
            config_path: options.config_path.clone(),
            config_write_block_reason: config_write_block_reason.clone(),
            current_config: Arc::new(Mutex::new(loaded.config)),
            apply_lock: Mutex::new(()),
            metadata: Arc::new(Mutex::new(RuntimeMetadata {
                phase: RuntimePhase::Ready,
                emergency_key,
                apply_reconciliation: ApplyReconciliation::Settled,
                last_error: config_write_block_reason,
            })),
            hook_thread_id: AtomicU32::new(hook_thread_id),
            rule_count: Arc::new(AtomicUsize::new(rule_count)),
            logger_shutdown,
            threads: Mutex::new(Some(RuntimeThreads { hook, logger })),
            reconciliation_thread: Mutex::new(None),
            marker,
        };
        Ok((
            runtime,
            StartReport {
                hook_thread_id,
                rule_count,
                emergency_key,
                source_schema_version,
                config_write_blocked,
                config_problems,
                previous_abnormal_termination,
            },
        ))
    }

    pub fn status(&self) -> RuntimeStatus {
        self.reap_reconciliation_thread();
        let metadata = self
            .metadata
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        RuntimeStatus {
            phase: metadata.phase,
            hook_thread_id: self.hook_thread_id.load(Ordering::Acquire),
            suspended: windows::is_suspended(),
            capture_active: windows::is_capture_active(),
            rule_count: self.rule_count.load(Ordering::Acquire),
            observed_events: windows::seq_count(),
            emergency_key: metadata.emergency_key,
            config_path: self.config_path.clone(),
            output_batches_sent: windows::output_sent(),
            output_batches_failed: windows::output_failed(),
            output_batches_dropped: windows::output_dropped(),
            callback_latency_us: windows::callback_latency_stats(),
            hold_delay_us: windows::hold_delay_stats(),
            state_revision: windows::state_revision(),
            apply_reconciliation: metadata.apply_reconciliation,
            last_error: metadata.last_error.clone(),
        }
    }

    pub fn current_config(&self) -> Config {
        self.reap_reconciliation_thread();
        self.current_config
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn pause(&self) -> Result<PauseReport, String> {
        self.require_ready()?;
        windows::suspend().inspect_err(|error| self.record_error(error))
    }

    pub fn resume(&self) -> Result<(), String> {
        self.require_ready()?;
        windows::resume().inspect_err(|error| self.record_error(error))
    }

    pub fn apply_config(&self, draft: Config) -> ApplyReport {
        let _guard = match self.apply_lock.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return ApplyReport::failure(
                    ApplyOutcome::RuntimeFailed,
                    "configuration apply lock is poisoned".to_string(),
                    true,
                );
            }
        };
        self.reap_reconciliation_thread();
        if let Err(error) = self.require_ready() {
            return ApplyReport::failure(ApplyOutcome::RuntimeFailed, error, true);
        }
        if let Some(reason) = self.config_write_block_reason.as_deref() {
            let error = format!("configuration apply refused: {reason}");
            self.record_error(&error);
            return ApplyReport::failure(ApplyOutcome::PersistenceFailed, error, false);
        }
        let reconciliation = self
            .metadata
            .lock()
            .map(|metadata| metadata.apply_reconciliation)
            .unwrap_or(ApplyReconciliation::RecoveryRequired { request_id: 0 });
        match reconciliation {
            ApplyReconciliation::Pending { request_id } => {
                return ApplyReport::failure_with_request(
                    ApplyOutcome::RuntimeBusy,
                    format!("rule replacement request {request_id} is still being reconciled"),
                    true,
                    Some(request_id),
                );
            }
            ApplyReconciliation::RecoveryRequired { request_id } => {
                return ApplyReport::failure_with_request(
                    ApplyOutcome::RuntimeBusy,
                    format!(
                        "rule replacement request {request_id} needs agent restart before another apply"
                    ),
                    true,
                    Some(request_id),
                );
            }
            _ => {}
        }
        let old = self.current_config();
        let (report, disposition) = apply_transaction(
            &self.config_path,
            &old,
            draft,
            save_with_report,
            |index, emergency_key, rule_count| match windows::replace_rules(
                index,
                emergency_key,
                rule_count,
            ) {
                windows::ReplaceRulesOutcome::Applied(report) => ReplaceAttempt::Applied(report),
                windows::ReplaceRulesOutcome::Cancelled(failure) => {
                    ReplaceAttempt::Cancelled(failure)
                }
                windows::ReplaceRulesOutcome::Failed(failure) => ReplaceAttempt::Failed(failure),
                windows::ReplaceRulesOutcome::OutcomeUnknown(pending) => {
                    ReplaceAttempt::OutcomeUnknown {
                        request_id: pending.request_id(),
                        pending,
                    }
                }
            },
        );
        match disposition {
            ApplyDisposition::None => {
                if let Some(error) = report.error.as_deref() {
                    self.record_error(error);
                }
            }
            ApplyDisposition::Applied(config) => {
                apply_live_config(
                    &self.current_config,
                    &self.metadata,
                    &self.rule_count,
                    config,
                    ApplyReconciliation::Settled,
                );
            }
            ApplyDisposition::Reconcile(pending) => {
                self.start_reconciliation(pending);
            }
        }
        report
    }

    pub fn begin_capture(&self, timeout: Duration) -> Result<CaptureSession, String> {
        self.require_ready()?;
        windows::begin_capture(timeout).inspect_err(|error| self.record_error(error))
    }

    pub fn cancel_capture(&self, session_id: u64) -> Result<(), String> {
        self.require_ready()?;
        windows::cancel_capture(session_id).inspect_err(|error| self.record_error(error))
    }

    fn start_reconciliation(&self, pending: PendingApply<windows::PendingReplace>) {
        let request_id = pending.request_id;
        if let Ok(mut metadata) = self.metadata.lock() {
            metadata.apply_reconciliation = ApplyReconciliation::Pending { request_id };
            metadata.last_error = Some(format!(
                "rule replacement request {request_id} timed out after starting; final outcome pending"
            ));
        }
        let config_path = self.config_path.clone();
        let current_config = Arc::clone(&self.current_config);
        let metadata = Arc::clone(&self.metadata);
        let rule_count = Arc::clone(&self.rule_count);
        let thread = std::thread::spawn(move || {
            reconcile_pending_apply(&config_path, current_config, metadata, rule_count, pending)
        });
        if let Ok(mut slot) = self.reconciliation_thread.lock()
            && let Some(previous) = slot.replace(thread)
        {
            let _ = previous.join();
        }
    }

    fn reap_reconciliation_thread(&self) {
        let finished = self
            .reconciliation_thread
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(JoinHandle::is_finished))
            .unwrap_or(false);
        if finished
            && let Some(thread) = self
                .reconciliation_thread
                .lock()
                .ok()
                .and_then(|mut slot| slot.take())
        {
            let _ = thread.join();
        }
    }

    pub fn shutdown(&self) -> ShutdownReport {
        if let Ok(mut metadata) = self.metadata.lock() {
            if metadata.phase == RuntimePhase::Stopped {
                return self.shutdown_snapshot(false, false, false);
            }
            metadata.phase = RuntimePhase::ShuttingDown;
        }

        let thread_id = self.hook_thread_id.swap(0, Ordering::AcqRel);
        let quit_posted = thread_id != 0 && windows::post_quit(thread_id);
        let threads = self
            .threads
            .lock()
            .ok()
            .and_then(|mut threads| threads.take());
        let (hook_thread_panicked, logger_thread_panicked) = if let Some(threads) = threads {
            let hook_thread_panicked = threads.hook.join().is_err();
            if let Some(reconciliation) = self
                .reconciliation_thread
                .lock()
                .ok()
                .and_then(|mut slot| slot.take())
            {
                let _ = reconciliation.join();
            }
            self.logger_shutdown.store(true, Ordering::Release);
            windows::clear_log_sender();
            let logger_thread_panicked = threads.logger.join().is_err();
            (hook_thread_panicked, logger_thread_panicked)
        } else {
            (false, false)
        };
        self.marker.clear();
        if let Ok(mut metadata) = self.metadata.lock() {
            metadata.phase = RuntimePhase::Stopped;
        }
        self.shutdown_snapshot(quit_posted, hook_thread_panicked, logger_thread_panicked)
    }

    fn shutdown_snapshot(
        &self,
        quit_posted: bool,
        hook_thread_panicked: bool,
        logger_thread_panicked: bool,
    ) -> ShutdownReport {
        ShutdownReport {
            quit_posted,
            hook_thread_panicked,
            logger_thread_panicked,
            observed_events: windows::seq_count(),
            output_batches_sent: windows::output_sent(),
            output_batches_failed: windows::output_failed(),
            output_batches_dropped: windows::output_dropped(),
        }
    }

    fn require_ready(&self) -> Result<(), String> {
        let phase = self
            .metadata
            .lock()
            .map_err(|_| "runtime metadata lock is poisoned".to_string())?
            .phase;
        if phase == RuntimePhase::Ready {
            Ok(())
        } else {
            Err(format!("runtime is not ready ({phase:?})"))
        }
    }

    fn record_error(&self, error: &str) {
        if let Ok(mut metadata) = self.metadata.lock() {
            metadata.last_error = Some(error.to_string());
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

impl ApplyReport {
    fn failure(outcome: ApplyOutcome, error: String, recovery_required: bool) -> Self {
        Self::failure_with_request(outcome, error, recovery_required, None)
    }

    fn failure_with_request(
        outcome: ApplyOutcome,
        error: String,
        recovery_required: bool,
        runtime_request_id: Option<u64>,
    ) -> Self {
        Self {
            outcome,
            rule_count: 0,
            validation_errors: Vec::new(),
            save: None,
            runtime: None,
            rollback: None,
            runtime_request_id,
            error: Some(error),
            recovery_required,
        }
    }
}

enum ReplaceAttempt<P> {
    Applied(ReplaceReport),
    Cancelled(ControlFailure),
    Failed(ControlFailure),
    OutcomeUnknown { request_id: u64, pending: P },
}

enum ApplyDisposition<P> {
    None,
    Applied(Config),
    Reconcile(PendingApply<P>),
}

struct PendingApply<P> {
    request_id: u64,
    old: Config,
    draft: Config,
    pending: P,
}

fn apply_transaction<P>(
    config_path: &Path,
    old: &Config,
    draft: Config,
    mut persist: impl FnMut(&Path, &Config) -> Result<SaveReport, String>,
    replace: impl FnOnce(RuleIndex, Key, usize) -> ReplaceAttempt<P>,
) -> (ApplyReport, ApplyDisposition<P>) {
    let validated = match validate_config(&draft) {
        Ok(validated) => validated,
        Err(errors) => {
            return (
                ApplyReport {
                    outcome: ApplyOutcome::ValidationFailed,
                    rule_count: 0,
                    validation_errors: errors.into_iter().map(|error| error.0).collect(),
                    save: None,
                    runtime: None,
                    rollback: None,
                    runtime_request_id: None,
                    error: None,
                    recovery_required: false,
                },
                ApplyDisposition::None,
            );
        }
    };
    let rule_count = validated.rules.len();
    let index = match RuleIndex::compile(validated.rules) {
        Ok(index) => index,
        Err(errors) => {
            return (
                ApplyReport {
                    outcome: ApplyOutcome::ValidationFailed,
                    rule_count,
                    validation_errors: errors.into_iter().map(|error| error.to_string()).collect(),
                    save: None,
                    runtime: None,
                    rollback: None,
                    runtime_request_id: None,
                    error: None,
                    recovery_required: false,
                },
                ApplyDisposition::None,
            );
        }
    };
    let save = match persist(config_path, &draft) {
        Ok(report) => report,
        Err(error) => {
            return (
                ApplyReport::failure(ApplyOutcome::PersistenceFailed, error, false),
                ApplyDisposition::None,
            );
        }
    };
    match replace(index, validated.emergency_key, rule_count) {
        ReplaceAttempt::Applied(runtime) => (
            ApplyReport {
                outcome: ApplyOutcome::Applied,
                rule_count,
                validation_errors: Vec::new(),
                save: Some(save),
                runtime: Some(runtime),
                rollback: None,
                runtime_request_id: None,
                error: None,
                recovery_required: false,
            },
            ApplyDisposition::Applied(draft),
        ),
        ReplaceAttempt::Cancelled(failure) | ReplaceAttempt::Failed(failure) => {
            let cancelled = failure.kind == ControlFailureKind::Cancelled;
            let rollback = persist(config_path, old);
            let rollback_error = rollback.as_ref().err().cloned();
            let rollback_failed = rollback_error.is_some();
            let combined = match rollback_error {
                Some(rollback_error) => {
                    format!(
                        "{}; failed to restore the previous config: {rollback_error}",
                        failure.message
                    )
                }
                None => failure.message,
            };
            let recovery_required = !cancelled || rollback_failed;
            (
                ApplyReport {
                    outcome: if cancelled {
                        ApplyOutcome::RuntimeCancelled
                    } else {
                        ApplyOutcome::RuntimeFailed
                    },
                    rule_count,
                    validation_errors: Vec::new(),
                    save: Some(save),
                    runtime: None,
                    rollback: rollback.ok(),
                    runtime_request_id: failure.request_id,
                    error: Some(combined),
                    recovery_required,
                },
                ApplyDisposition::None,
            )
        }
        ReplaceAttempt::OutcomeUnknown {
            request_id,
            pending,
        } => (
            ApplyReport {
                outcome: ApplyOutcome::RuntimeOutcomeUnknown,
                rule_count,
                validation_errors: Vec::new(),
                save: Some(save),
                runtime: None,
                rollback: None,
                runtime_request_id: Some(request_id),
                error: Some(format!(
                    "rule replacement request {request_id} started but did not finish within the acknowledgement deadline; reconciliation is pending"
                )),
                recovery_required: true,
            },
            ApplyDisposition::Reconcile(PendingApply {
                request_id,
                old: old.clone(),
                draft,
                pending,
            }),
        ),
    }
}

fn apply_live_config(
    current_config: &Arc<Mutex<Config>>,
    metadata: &Arc<Mutex<RuntimeMetadata>>,
    rule_count: &Arc<AtomicUsize>,
    config: Config,
    reconciliation: ApplyReconciliation,
) {
    let validated = validate_config(&config).ok();
    let enabled_rule_count = validated
        .as_ref()
        .map(|validated| validated.rules.len())
        .unwrap_or(0);
    rule_count.store(enabled_rule_count, Ordering::Release);
    if let Ok(mut current) = current_config.lock() {
        *current = config;
    }
    if let Ok(mut metadata) = metadata.lock() {
        if let Some(validated) = validated {
            metadata.emergency_key = validated.emergency_key;
        }
        metadata.apply_reconciliation = reconciliation;
        metadata.last_error = None;
    }
}

fn reconcile_pending_apply(
    config_path: &Path,
    current_config: Arc<Mutex<Config>>,
    metadata: Arc<Mutex<RuntimeMetadata>>,
    rule_count: Arc<AtomicUsize>,
    pending: PendingApply<windows::PendingReplace>,
) {
    reconcile_pending_apply_with(
        config_path,
        current_config,
        metadata,
        rule_count,
        pending,
        windows::PendingReplace::wait,
    );
}

fn reconcile_pending_apply_with<P>(
    config_path: &Path,
    current_config: Arc<Mutex<Config>>,
    metadata: Arc<Mutex<RuntimeMetadata>>,
    rule_count: Arc<AtomicUsize>,
    pending: PendingApply<P>,
    wait: impl FnOnce(P) -> windows::PendingReplaceOutcome,
) {
    let request_id = pending.request_id;
    match wait(pending.pending) {
        windows::PendingReplaceOutcome::Applied(_) => apply_live_config(
            &current_config,
            &metadata,
            &rule_count,
            pending.draft,
            ApplyReconciliation::AppliedAfterTimeout { request_id },
        ),
        windows::PendingReplaceOutcome::Failed(error) => {
            match save_with_report(config_path, &pending.old) {
                Ok(rollback) => {
                    let warning = if rollback.cleanup_warnings.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; rollback cleanup warnings: {}",
                            rollback.cleanup_warnings.join("; ")
                        )
                    };
                    if let Ok(mut metadata) = metadata.lock() {
                        metadata.apply_reconciliation =
                            ApplyReconciliation::RolledBackAfterTimeout { request_id };
                        metadata.last_error = Some(format!(
                            "late rule replacement request {request_id} failed and the previous config was restored: {error}{warning}"
                        ));
                    }
                }
                Err(rollback_error) => {
                    if let Ok(mut metadata) = metadata.lock() {
                        metadata.apply_reconciliation =
                            ApplyReconciliation::RecoveryRequired { request_id };
                        metadata.last_error = Some(format!(
                            "late rule replacement request {request_id} failed ({error}); restoring the previous config also failed: {rollback_error}"
                        ));
                    }
                }
            }
        }
        windows::PendingReplaceOutcome::OutcomeUnknown(error) => {
            if let Ok(mut metadata) = metadata.lock() {
                metadata.apply_reconciliation =
                    ApplyReconciliation::RecoveryRequired { request_id };
                metadata.last_error = Some(format!(
                    "rule replacement request {request_id} never produced a final result ({error}); persisted draft retained, restart the agent to recover deterministically"
                ));
            }
        }
    }
}

fn logger_loop(receiver: mpsc::Receiver<String>, shutdown: Arc<AtomicBool>, sink: DiagnosticSink) {
    loop {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => sink(line),
            Err(RecvTimeoutError::Timeout) => {
                if shutdown.load(Ordering::Acquire) {
                    while let Ok(line) = receiver.try_recv() {
                        sink(line);
                    }
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

struct CrashMarker {
    path: PathBuf,
}

impl CrashMarker {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn was_left_running(&self) -> bool {
        self.path.exists()
    }

    fn mark_running(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create runtime data directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        fs::write(&self.path, format!("{}\n", std::process::id())).map_err(|error| {
            format!(
                "failed to write crash marker `{}`: {error}",
                self.path.display()
            )
        })
    }

    fn clear(&self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn default_data_dir() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("InputFlow")
}

pub fn current_schema_version() -> u32 {
    SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use inputflow_config::{ActionConfig, KeyConfig, RuleConfig, TriggerConfig, default_config};

    fn config_with_rule(id: &str) -> Config {
        let mut config = default_config();
        config.rules.push(RuleConfig {
            id: id.to_string(),
            enabled: true,
            trigger: TriggerConfig::KeyChord {
                first: KeyConfig::logical(Key::A),
                second: KeyConfig::logical(Key::B),
            },
            action: ActionConfig::KeyChord {
                keys: vec![KeyConfig::logical(Key::C)],
            },
        });
        config
    }

    fn replace_report(rule_count: usize) -> ReplaceReport {
        ReplaceReport {
            pause: PauseReport {
                held_events: 0,
                requested_inputs: 0,
                inserted_inputs: 0,
                output_complete: true,
                last_error: 0,
            },
            was_suspended: false,
            rule_count,
            emergency_key: Key::F12,
        }
    }

    #[test]
    fn invalid_apply_neither_persists_nor_replaces() {
        let old = default_config();
        let mut invalid = config_with_rule("bad");
        invalid.schema_version = 99;
        let mut persisted = false;
        let mut replaced = false;
        let (report, applied) = apply_transaction::<()>(
            Path::new("ignored.json"),
            &old,
            invalid,
            |_, _| {
                persisted = true;
                unreachable!()
            },
            |_, _, _| {
                replaced = true;
                unreachable!()
            },
        );
        assert_eq!(report.outcome, ApplyOutcome::ValidationFailed);
        assert!(!persisted);
        assert!(!replaced);
        assert!(matches!(applied, ApplyDisposition::None));
    }

    #[test]
    fn successful_apply_persists_before_runtime_replace() {
        let old = default_config();
        let draft = config_with_rule("new");
        let order = Arc::new(Mutex::new(Vec::new()));
        let persist_order = Arc::clone(&order);
        let replace_order = Arc::clone(&order);
        let (report, applied) = apply_transaction(
            Path::new("config.json"),
            &old,
            draft.clone(),
            move |path, _| {
                persist_order.lock().unwrap().push("persist");
                Ok(SaveReport {
                    path: path.to_path_buf(),
                    backup_path: None,
                    cleanup_warnings: vec!["cleanup warning".to_string()],
                })
            },
            move |_, _, rule_count| {
                replace_order.lock().unwrap().push("replace");
                ReplaceAttempt::<()>::Applied(replace_report(rule_count))
            },
        );
        assert_eq!(report.outcome, ApplyOutcome::Applied);
        assert_eq!(*order.lock().unwrap(), vec!["persist", "replace"]);
        assert!(matches!(applied, ApplyDisposition::Applied(config) if config == draft));
        assert_eq!(
            report.save.unwrap().cleanup_warnings,
            vec!["cleanup warning"]
        );
    }

    #[test]
    fn disabled_rules_are_persisted_but_not_compiled_into_the_runtime() {
        let old = default_config();
        let mut draft = config_with_rule("disabled");
        draft.rules[0].enabled = false;
        let persisted = Arc::new(Mutex::new(None));
        let persisted_copy = Arc::clone(&persisted);

        let (report, applied) = apply_transaction(
            Path::new("config.json"),
            &old,
            draft.clone(),
            move |path, config| {
                *persisted_copy.lock().unwrap() = Some(config.clone());
                Ok(SaveReport {
                    path: path.to_path_buf(),
                    backup_path: None,
                    cleanup_warnings: Vec::new(),
                })
            },
            |index, _, rule_count| {
                assert!(index.is_empty());
                assert_eq!(rule_count, 0);
                ReplaceAttempt::<()>::Applied(replace_report(rule_count))
            },
        );

        assert_eq!(report.outcome, ApplyOutcome::Applied);
        assert_eq!(report.rule_count, 0);
        assert_eq!(*persisted.lock().unwrap(), Some(draft.clone()));
        assert!(matches!(applied, ApplyDisposition::Applied(config) if config == draft));
    }

    #[test]
    fn failed_runtime_replace_attempts_old_config_rollback() {
        let old = default_config();
        let draft = config_with_rule("new");
        let persisted = Arc::new(Mutex::new(Vec::<Config>::new()));
        let persisted_copy = Arc::clone(&persisted);
        let (report, applied) = apply_transaction(
            Path::new("config.json"),
            &old,
            draft.clone(),
            move |path, config| {
                persisted_copy.lock().unwrap().push(config.clone());
                Ok(SaveReport {
                    path: path.to_path_buf(),
                    backup_path: None,
                    cleanup_warnings: Vec::new(),
                })
            },
            |_, _, _| {
                ReplaceAttempt::<()>::Failed(ControlFailure {
                    kind: ControlFailureKind::Failed,
                    request_id: Some(7),
                    message: "injected runtime failure".to_string(),
                })
            },
        );
        assert_eq!(report.outcome, ApplyOutcome::RuntimeFailed);
        assert!(report.recovery_required);
        assert!(report.rollback.is_some());
        assert!(matches!(applied, ApplyDisposition::None));
        assert_eq!(*persisted.lock().unwrap(), vec![draft, old]);
    }

    #[test]
    fn cancelled_runtime_replace_is_structured_and_safely_rolled_back() {
        let old = default_config();
        let draft = config_with_rule("new");
        let persisted = Arc::new(Mutex::new(Vec::<Config>::new()));
        let persisted_copy = Arc::clone(&persisted);
        let (report, disposition) = apply_transaction(
            Path::new("config.json"),
            &old,
            draft.clone(),
            move |path, config| {
                persisted_copy.lock().unwrap().push(config.clone());
                Ok(SaveReport {
                    path: path.to_path_buf(),
                    backup_path: None,
                    cleanup_warnings: Vec::new(),
                })
            },
            |_, _, _| {
                ReplaceAttempt::<()>::Cancelled(ControlFailure {
                    kind: ControlFailureKind::Cancelled,
                    request_id: Some(8),
                    message: "injected cancellation".to_string(),
                })
            },
        );
        assert_eq!(report.outcome, ApplyOutcome::RuntimeCancelled);
        assert_eq!(report.runtime_request_id, Some(8));
        assert!(!report.recovery_required);
        assert!(matches!(disposition, ApplyDisposition::None));
        assert_eq!(*persisted.lock().unwrap(), vec![draft, old]);
    }

    #[test]
    fn delayed_success_after_timeout_never_rolls_disk_back_to_old_config() {
        let old = default_config();
        let draft = config_with_rule("late-success");
        let persisted = Arc::new(Mutex::new(Vec::<Config>::new()));
        let persisted_copy = Arc::clone(&persisted);
        let (late_tx, late_rx) = mpsc::channel();
        let (report, disposition) = apply_transaction(
            Path::new("config.json"),
            &old,
            draft.clone(),
            move |path, config| {
                persisted_copy.lock().unwrap().push(config.clone());
                Ok(SaveReport {
                    path: path.to_path_buf(),
                    backup_path: None,
                    cleanup_warnings: Vec::new(),
                })
            },
            |_, _, _| ReplaceAttempt::OutcomeUnknown {
                request_id: 42,
                pending: late_rx,
            },
        );
        assert_eq!(report.outcome, ApplyOutcome::RuntimeOutcomeUnknown);
        assert_eq!(report.runtime_request_id, Some(42));
        assert!(report.rollback.is_none());
        assert_eq!(*persisted.lock().unwrap(), vec![draft.clone()]);

        let ApplyDisposition::Reconcile(pending) = disposition else {
            panic!("unknown completion must be reconciled");
        };
        let current = Arc::new(Mutex::new(old));
        let metadata = Arc::new(Mutex::new(RuntimeMetadata {
            phase: RuntimePhase::Ready,
            emergency_key: Key::F12,
            apply_reconciliation: ApplyReconciliation::Pending { request_id: 42 },
            last_error: None,
        }));
        let rule_count = Arc::new(AtomicUsize::new(0));

        late_tx
            .send(windows::PendingReplaceOutcome::Applied(replace_report(1)))
            .unwrap();
        reconcile_pending_apply_with(
            Path::new("unused-on-success.json"),
            Arc::clone(&current),
            Arc::clone(&metadata),
            Arc::clone(&rule_count),
            pending,
            |receiver| receiver.recv().unwrap(),
        );

        assert_eq!(*current.lock().unwrap(), draft.clone());
        assert_eq!(rule_count.load(Ordering::Acquire), 1);
        assert_eq!(
            metadata.lock().unwrap().apply_reconciliation,
            ApplyReconciliation::AppliedAfterTimeout { request_id: 42 }
        );
        assert_eq!(*persisted.lock().unwrap(), vec![draft]);
    }
}
