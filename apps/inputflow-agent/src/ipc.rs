use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use inputflow_config::{SaveReport, validate_config};
use inputflow_protocol::{
    ApplyResult, BeginCaptureParams, BeginCaptureResult, CAPTURE_TIMEOUT_MAX_MS,
    CAPTURE_TIMEOUT_MIN_MS, CLIENT_RESPONSE_TIMEOUT_MS, CancelCaptureParams, CancelCaptureResult,
    CaptureOutcomeDto, CapturedInputDto, ConfigParams, ConfigResult, EVENT_QUEUE_CAPACITY,
    EmptyParams, ErrorBody, EventHub, EventPayload, HandlerReply, HandshakeParams, HandshakeResult,
    MAX_FRAME_BYTES, MAX_REQUESTS_PER_CONNECTION, PROTOCOL_VERSION, PauseResult, PercentilesDto,
    PhysicalKeyDto, ProtocolHandler, ProtocolServer, ProtocolServerHandle, ResumeResult,
    RuntimeStageDto, SaveStageDto, StatsResult, StatusResult, ValidationResult, parse_params,
    to_value,
};
use inputflow_runtime::{
    ApplyOutcome, ApplyReconciliation, ApplyReport, CaptureOutcome, CapturedInput, PauseReport,
    Percentiles, ReplaceReport, Runtime, RuntimePhase, RuntimeStatus, current_schema_version,
};
use inputflow_windows::platform::pipe::default_pipe_name;

use super::AgentLog;

const STATUS_WATCH_INTERVAL: Duration = Duration::from_millis(50);

pub struct AgentIpc {
    pipe_name: String,
    server: Option<ProtocolServerHandle>,
    watcher_stop: Arc<AtomicBool>,
    watcher: Option<JoinHandle<()>>,
}

impl AgentIpc {
    pub fn start(runtime: Arc<Runtime>, log: Arc<AgentLog>) -> Result<Self, String> {
        let pipe_name = default_pipe_name()?;
        let events = Arc::new(EventHub::new());
        let handler = Arc::new(AgentProtocolHandler {
            runtime: Arc::clone(&runtime),
            events: Arc::clone(&events),
            capture_owner: Arc::new(Mutex::new(None)),
            log,
        });
        let server = ProtocolServer::start(pipe_name.clone(), handler)?;

        let watcher_stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&watcher_stop);
        let watcher = thread::spawn(move || {
            let mut revision = runtime.status().state_revision;
            while !thread_stop.load(Ordering::Acquire) {
                thread::sleep(STATUS_WATCH_INTERVAL);
                let status = runtime.status();
                if status.state_revision != revision {
                    revision = status.state_revision;
                    events.publish(EventPayload::StatusChanged {
                        state_revision: status.state_revision,
                        suspended: status.suspended,
                        capture_active: status.capture_active,
                    });
                }
            }
        });
        Ok(Self {
            pipe_name,
            server: Some(server),
            watcher_stop,
            watcher: Some(watcher),
        })
    }

    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        let server_result = if let Some(mut server) = self.server.take() {
            server.shutdown()
        } else {
            Ok(())
        };
        self.watcher_stop.store(true, Ordering::Release);
        if let Some(watcher) = self.watcher.take() {
            watcher
                .join()
                .map_err(|_| "IPC status watcher thread panicked".to_string())?;
        }
        server_result
    }
}

impl Drop for AgentIpc {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

struct AgentProtocolHandler {
    runtime: Arc<Runtime>,
    events: Arc<EventHub>,
    capture_owner: Arc<Mutex<Option<CaptureOwner>>>,
    log: Arc<AgentLog>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CaptureOwner {
    connection_id: u64,
    session_id: u64,
}

impl ProtocolHandler for AgentProtocolHandler {
    fn handshake(&self, params: HandshakeParams) -> Result<HandshakeResult, ErrorBody> {
        if params.client_name.trim().is_empty() || params.client_name.len() > 64 {
            return Err(ErrorBody::new(
                "invalid_client",
                "client_name must contain 1 to 64 UTF-8 bytes",
            ));
        }
        if params.client_version.trim().is_empty() || params.client_version.len() > 32 {
            return Err(ErrorBody::new(
                "invalid_client",
                "client_version must contain 1 to 32 UTF-8 bytes",
            ));
        }
        Ok(HandshakeResult {
            server_name: "inputflow-agent".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            schema_version: current_schema_version(),
            capabilities: vec![
                "status".to_string(),
                "config_v4".to_string(),
                "validate_apply".to_string(),
                "pause_resume".to_string(),
                "stats".to_string(),
                "capture".to_string(),
                "bounded_events".to_string(),
            ],
            max_frame_bytes: MAX_FRAME_BYTES,
            max_requests_per_connection: MAX_REQUESTS_PER_CONNECTION,
            event_queue_capacity: EVENT_QUEUE_CAPACITY,
            client_response_timeout_ms: CLIENT_RESPONSE_TIMEOUT_MS,
        })
    }

    fn handle(
        &self,
        connection_id: u64,
        method: &str,
        params: serde_json::Value,
    ) -> Result<HandlerReply, ErrorBody> {
        let response = match method {
            "get_status" => {
                parse_params::<EmptyParams>(params)?;
                to_value(status_dto(&self.runtime.status()))?
            }
            "get_config" => {
                parse_params::<EmptyParams>(params)?;
                to_value(ConfigResult {
                    config: self.runtime.current_config(),
                })?
            }
            "validate_config" => {
                let params = parse_params::<ConfigParams>(params)?;
                let result = match validate_config(&params.config) {
                    Ok(validated) => ValidationResult {
                        valid: true,
                        rule_count: validated.rules.len(),
                        errors: Vec::new(),
                    },
                    Err(errors) => ValidationResult {
                        valid: false,
                        rule_count: 0,
                        errors: errors.into_iter().map(|error| error.0).collect(),
                    },
                };
                to_value(result)?
            }
            "apply_config" => {
                let params = parse_params::<ConfigParams>(params)?;
                let report = self.runtime.apply_config(params.config);
                let result = apply_dto(&report);
                self.events.publish(EventPayload::ConfigApplied {
                    outcome: result.outcome.clone(),
                    rule_count: result.rule_count,
                    recovery_required: result.recovery_required,
                });
                to_value(result)?
            }
            "pause" => {
                parse_params::<EmptyParams>(params)?;
                let report = self.runtime.pause().map_err(runtime_error)?;
                to_value(pause_dto(report, true))?
            }
            "resume" => {
                parse_params::<EmptyParams>(params)?;
                self.runtime.resume().map_err(runtime_error)?;
                to_value(ResumeResult { suspended: false })?
            }
            "get_stats" => {
                parse_params::<EmptyParams>(params)?;
                to_value(stats_dto(&self.runtime.status()))?
            }
            "begin_capture" => {
                let params = parse_params::<BeginCaptureParams>(params)?;
                return self.begin_capture(connection_id, params);
            }
            "cancel_capture" => {
                let params = parse_params::<CancelCaptureParams>(params)?;
                return self.cancel_capture(connection_id, params);
            }
            "subscribe_events" => {
                parse_params::<EmptyParams>(params)?;
                return self.events.subscribe().map(HandlerReply::Subscription);
            }
            _ => {
                return Err(ErrorBody::new(
                    "unknown_method",
                    format!("unknown protocol method `{method}`"),
                ));
            }
        };
        Ok(HandlerReply::Response(response))
    }

    fn disconnected(&self, connection_id: u64) {
        let session_id = self.capture_owner.lock().ok().and_then(|mut slot| {
            if slot.is_some_and(|owner| owner.connection_id == connection_id) {
                slot.take().map(|owner| owner.session_id)
            } else {
                None
            }
        });
        if let Some(session_id) = session_id {
            let _ = self.runtime.cancel_capture(session_id);
            self.log.write(
                "INFO",
                &format!(
                    "ipc_capture_disconnect: connection_id={connection_id} session_id={session_id}"
                ),
            );
        }
    }

    fn server_shutting_down(&self) {
        self.events.publish(EventPayload::ServerShuttingDown);
    }
}

impl AgentProtocolHandler {
    fn begin_capture(
        &self,
        connection_id: u64,
        params: BeginCaptureParams,
    ) -> Result<HandlerReply, ErrorBody> {
        if !(CAPTURE_TIMEOUT_MIN_MS..=CAPTURE_TIMEOUT_MAX_MS).contains(&params.timeout_ms) {
            return Err(ErrorBody::new(
                "invalid_capture_timeout",
                format!(
                    "timeout_ms must be in the range {CAPTURE_TIMEOUT_MIN_MS}..={CAPTURE_TIMEOUT_MAX_MS}"
                ),
            ));
        }
        let mut owner = self.capture_owner.lock().map_err(|_| {
            ErrorBody::new("internal_error", "capture ownership state is unavailable")
        })?;
        if let Some(active) = *owner {
            return Err(ErrorBody::new(
                "capture_busy",
                format!("capture session {} is already active", active.session_id),
            ));
        }
        let timeout = Duration::from_millis(params.timeout_ms);
        let session = self.runtime.begin_capture(timeout).map_err(capture_error)?;
        let session_id = session.id();
        *owner = Some(CaptureOwner {
            connection_id,
            session_id,
        });
        drop(owner);

        let capture_owner = Arc::clone(&self.capture_owner);
        let events = Arc::clone(&self.events);
        let runtime = Arc::clone(&self.runtime);
        let log = Arc::clone(&self.log);
        thread::spawn(move || {
            let outcome = session
                .recv_timeout(timeout + Duration::from_secs(2))
                .unwrap_or_else(|error| {
                    let _ = runtime.cancel_capture(session_id);
                    log.write(
                        "ERROR",
                        &format!(
                            "ipc_capture_terminal_missing: session_id={session_id} error={error}"
                        ),
                    );
                    CaptureOutcome::Shutdown
                });
            if let Ok(mut owner) = capture_owner.lock()
                && owner.is_some_and(|owner| owner.session_id == session_id)
            {
                *owner = None;
            }
            events.publish(EventPayload::CaptureCompleted {
                session_id,
                outcome: capture_outcome_dto(outcome),
            });
        });
        Ok(HandlerReply::Response(to_value(BeginCaptureResult {
            session_id,
            timeout_ms: params.timeout_ms,
        })?))
    }

    fn cancel_capture(
        &self,
        connection_id: u64,
        params: CancelCaptureParams,
    ) -> Result<HandlerReply, ErrorBody> {
        let owner = self.capture_owner.lock().map_err(|_| {
            ErrorBody::new("internal_error", "capture ownership state is unavailable")
        })?;
        match *owner {
            Some(active)
                if active.connection_id == connection_id
                    && active.session_id == params.session_id => {}
            Some(active) if active.session_id == params.session_id => {
                return Err(ErrorBody::new(
                    "capture_not_owned",
                    "capture can only be cancelled by its owning connection",
                ));
            }
            Some(active) => {
                return Err(ErrorBody::new(
                    "capture_session_mismatch",
                    format!(
                        "capture session {} is active, not {}",
                        active.session_id, params.session_id
                    ),
                ));
            }
            None => {
                return Err(ErrorBody::new(
                    "capture_not_active",
                    "no capture session is active",
                ));
            }
        }
        drop(owner);
        self.runtime
            .cancel_capture(params.session_id)
            .map_err(capture_error)?;
        Ok(HandlerReply::Response(to_value(CancelCaptureResult {
            session_id: params.session_id,
            cancelled: true,
        })?))
    }
}

fn runtime_error(message: String) -> ErrorBody {
    ErrorBody::new("runtime_error", message)
}

fn capture_error(message: String) -> ErrorBody {
    let code = if message.contains("already active") {
        "capture_busy"
    } else if message.contains("not ready") {
        "runtime_unavailable"
    } else {
        "capture_error"
    };
    ErrorBody::new(code, message)
}

fn status_dto(status: &RuntimeStatus) -> StatusResult {
    let (apply_reconciliation, reconciliation_request_id) = match status.apply_reconciliation {
        ApplyReconciliation::Settled => ("settled", None),
        ApplyReconciliation::Pending { request_id } => ("pending", Some(request_id)),
        ApplyReconciliation::AppliedAfterTimeout { request_id } => {
            ("applied_after_timeout", Some(request_id))
        }
        ApplyReconciliation::RolledBackAfterTimeout { request_id } => {
            ("rolled_back_after_timeout", Some(request_id))
        }
        ApplyReconciliation::RecoveryRequired { request_id } => {
            ("recovery_required", Some(request_id))
        }
    };
    StatusResult {
        phase: match status.phase {
            RuntimePhase::Ready => "ready",
            RuntimePhase::ShuttingDown => "shutting_down",
            RuntimePhase::Stopped => "stopped",
        }
        .to_string(),
        suspended: status.suspended,
        capture_active: status.capture_active,
        rule_count: status.rule_count,
        emergency_key: status.emergency_key.to_string(),
        state_revision: status.state_revision,
        apply_reconciliation: apply_reconciliation.to_string(),
        reconciliation_request_id,
        last_error: status.last_error.clone(),
    }
}

fn stats_dto(status: &RuntimeStatus) -> StatsResult {
    StatsResult {
        observed_events: status.observed_events,
        output_batches_sent: status.output_batches_sent,
        output_batches_failed: status.output_batches_failed,
        output_batches_dropped: status.output_batches_dropped,
        callback_latency_us: status.callback_latency_us.map(percentiles_dto),
        hold_delay_us: status.hold_delay_us.map(percentiles_dto),
    }
}

fn percentiles_dto((samples, p50, p95, p99, max): Percentiles) -> PercentilesDto {
    PercentilesDto {
        samples,
        p50,
        p95,
        p99,
        max,
    }
}

fn pause_dto(report: PauseReport, suspended: bool) -> PauseResult {
    PauseResult {
        suspended,
        held_events: report.held_events,
        requested_inputs: report.requested_inputs,
        inserted_inputs: report.inserted_inputs,
        output_complete: report.output_complete,
        last_error: report.last_error,
    }
}

fn save_dto(report: &SaveReport) -> SaveStageDto {
    SaveStageDto {
        committed: true,
        backup_created: report.backup_path.is_some(),
        cleanup_warnings: report.cleanup_warnings.clone(),
    }
}

fn runtime_stage_dto(report: ReplaceReport) -> RuntimeStageDto {
    RuntimeStageDto {
        replaced: true,
        was_suspended: report.was_suspended,
        rule_count: report.rule_count,
        emergency_key: report.emergency_key.to_string(),
        pause: pause_dto(report.pause, true),
    }
}

fn apply_dto(report: &ApplyReport) -> ApplyResult {
    ApplyResult {
        outcome: match report.outcome {
            ApplyOutcome::Applied => "applied",
            ApplyOutcome::ValidationFailed => "validation_failed",
            ApplyOutcome::PersistenceFailed => "persistence_failed",
            ApplyOutcome::RuntimeCancelled => "runtime_cancelled",
            ApplyOutcome::RuntimeFailed => "runtime_failed",
            ApplyOutcome::RuntimeOutcomeUnknown => "runtime_outcome_unknown",
            ApplyOutcome::RuntimeBusy => "runtime_busy",
        }
        .to_string(),
        rule_count: report.rule_count,
        validation_errors: report.validation_errors.clone(),
        save: report.save.as_ref().map(save_dto),
        runtime: report.runtime.map(runtime_stage_dto),
        rollback: report.rollback.as_ref().map(save_dto),
        runtime_request_id: report.runtime_request_id,
        error: report.error.clone(),
        recovery_required: report.recovery_required,
    }
}

fn capture_outcome_dto(outcome: CaptureOutcome) -> CaptureOutcomeDto {
    match outcome {
        CaptureOutcome::Captured(input) => CaptureOutcomeDto::Captured {
            input: match input {
                CapturedInput::Key { logical, physical } => CapturedInputDto::Key {
                    logical: logical.to_string(),
                    physical: physical.and_then(|key| match key {
                        inputflow_engine::Key::Physical {
                            scan_code,
                            extended,
                        } => Some(PhysicalKeyDto {
                            scan_code,
                            extended,
                        }),
                        _ => None,
                    }),
                },
                CapturedInput::MouseButton(button) => CapturedInputDto::MouseButton {
                    button: button.to_string(),
                },
            },
        },
        CaptureOutcome::Cancelled => CaptureOutcomeDto::Cancelled,
        CaptureOutcome::TimedOut => CaptureOutcomeDto::TimedOut,
        CaptureOutcome::Shutdown => CaptureOutcomeDto::Shutdown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inputflow_runtime::RuntimePhase;

    #[test]
    fn status_and_apply_enums_have_stable_wire_names() {
        let status = RuntimeStatus {
            phase: RuntimePhase::Ready,
            hook_thread_id: 1,
            suspended: false,
            capture_active: false,
            rule_count: 2,
            observed_events: 3,
            emergency_key: inputflow_engine::Key::F12,
            config_path: "ignored.json".into(),
            output_batches_sent: 4,
            output_batches_failed: 0,
            output_batches_dropped: 0,
            callback_latency_us: None,
            hold_delay_us: None,
            state_revision: 5,
            apply_reconciliation: ApplyReconciliation::Settled,
            last_error: None,
        };
        let wire = status_dto(&status);
        assert_eq!(wire.phase, "ready");
        assert_eq!(wire.apply_reconciliation, "settled");

        let report = ApplyReport {
            outcome: ApplyOutcome::ValidationFailed,
            rule_count: 0,
            validation_errors: vec!["bad".to_string()],
            save: None,
            runtime: None,
            rollback: None,
            runtime_request_id: None,
            error: None,
            recovery_required: false,
        };
        assert_eq!(apply_dto(&report).outcome, "validation_failed");
    }
}
