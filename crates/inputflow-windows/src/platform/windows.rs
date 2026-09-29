//! Windows platform layer: low-level keyboard/mouse hooks, the message loop,
//! engine integration, and `SendInput` output.
//!
//! # Safety invariants
//!
//! - All `unsafe` Win32 calls and thread lifecycle are confined to this module.
//! - Hook callbacks dereference `lparam` only when `code >= HC_ACTION`, where the
//!   OS guarantees it points to a valid `KBDLLHOOKSTRUCT` / `MSLLHOOKSTRUCT`.
//! - Callbacks only normalize the event, take a lock on the matcher, and perform
//!   a tagged `SendInput` for a resolved replay/action. They never touch disk,
//!   the network, or the GUI. Synchronous output preserves ordering but has no
//!   documented worst-case duration; callback wall time (including downstream
//!   hooks) is sampled and this remains a Windows performance acceptance item.
//! - Hook handles are owned by the hook thread and are uninstalled before that
//!   thread returns. The hook thread creates its message queue (via
//!   `PeekMessageW`) before reporting its thread id, so `PostThreadMessageW`
//!   cannot race the queue creation.
//!
//! M5 behavior: the matcher runs synchronously in the callback; a suppressed
//! event returns non-zero, a passed-through event is forwarded down the chain.
//! Matches emit an action and failures replay the held events, both via
//! synchronous `SendInput` (with a `dwExtraInfo` tag so our own injected events
//! are recognized and never re-trigger matching). Doing so in the callback keeps
//! replay/action ordered before any subsequent pass-through event. A thread timer
//! (`SetTimer`) drives `Hold` / `Hold+MouseButton` deadlines via `WM_TIMER`.

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use inputflow_engine::{
    Action, Command, Decision, InputEvent, InputSource, Key, Matcher, MouseButton, MouseKind,
    PercentileSnapshot, PercentileSummary, PercentileTracker, Resolution,
};

use windows_sys::Win32::Foundation::{GetLastError, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, KillTimer,
    LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP,
    WM_QUIT, WM_TIMER,
};

use crate::keymap;

/// Unique `dwExtraInfo` tag written into input synthesized by this program.
pub const SELF_EXTRA_INFO_TAG: usize = 0x494E_5055; // ASCII "INPU"

/// X-button identifiers carried in `MOUSEINPUT.mouseData`.
const XBUTTON1: u32 = 1;
const XBUTTON2: u32 = 2;

/// Polling interval in milliseconds for deadline checks. `SetTimer` clamps any
/// smaller value to USER_TIMER_MINIMUM (10 ms), and WM_TIMER remains low priority.
const TIMEOUT_TIMER_INTERVAL_MS: u32 = 10;
/// Bounded grace period on clean shutdown for physical releases whose downs
/// were consumed. This avoids most orphan-up exits without hanging forever if a
/// device disappears or an up event never arrives.
const SHUTDOWN_TOMBSTONE_DRAIN_MS: u64 = 2_000;
/// Maximum time an external control caller waits for the hook thread to finish
/// a pause/resume transaction. A still-queued request is cancelled on timeout.
const CONTROL_ACK_TIMEOUT_MS: u64 = 2_000;
/// Private thread message used only to wake the hook thread for queued controls.
const WM_INPUTFLOW_CONTROL: u32 = WM_APP + 1;
const CONTROL_PENDING: u8 = 0;
const CONTROL_RUNNING: u8 = 1;
const CONTROL_CANCELLED: u8 = 2;
const CONTROL_COMPLETE: u8 = 3;

/// The matcher, shared with the hook thread and driven synchronously from the
/// callbacks. Installed once before the hook thread starts.
static MATCHER: OnceLock<Mutex<Matcher>> = OnceLock::new();
/// Thread id of the live hook/message-loop owner, or zero outside its lifetime.
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
/// Control requests are executed only by the hook thread. This makes pause,
/// replay, resume, timer resolution, and physical callbacks one serial stream.
static CONTROL_REQUESTS: OnceLock<Mutex<VecDeque<ControlRequest>>> = OnceLock::new();
static CONTROL_SEQUENCE: AtomicU64 = AtomicU64::new(0);
/// Sender for human-readable diagnostics consumed by the logger thread.
static LOG_TX: OnceLock<Mutex<Option<SyncSender<String>>>> = OnceLock::new();
/// When `true`, no input is intercepted (emergency bypass / output failure).
static BYPASS: AtomicBool = AtomicBool::new(false);
/// Monotonic revision for consumers that publish runtime status (tray now,
/// IPC later). It advances only when the authoritative bypass value changes.
static STATE_REVISION: AtomicU64 = AtomicU64::new(0);
/// Clean shutdown is draining consumed releases; emergency toggles are disabled.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
/// Monotonic sequence number assigned to every normalized event.
static SEQ: AtomicU64 = AtomicU64::new(0);
/// Successful `SendInput` output batches.
static OUTPUT_SENT: AtomicU64 = AtomicU64::new(0);
/// Failed `SendInput` output batches (inserted fewer events than requested).
static OUTPUT_FAILED: AtomicU64 = AtomicU64::new(0);
/// Output commands dropped (retained for API compatibility; synchronous output
/// no longer drops commands — failures instead enter bypass).
static OUTPUT_DROPPED: AtomicU64 = AtomicU64::new(0);
/// One-based index into `Key::NAMED`; zero selects the default `F12`.
static EMERGENCY_KEY_INDEX: AtomicU32 = AtomicU32::new(0);
/// Whether per-event debug logging is enabled (off by default; NFR-05).
static DEBUG_LOG: AtomicBool = AtomicBool::new(false);
/// Reservoir of matcher-decision latency samples, in microseconds.
static CALLBACK_LATENCY: OnceLock<Mutex<PercentileTracker>> = OnceLock::new();
/// Reservoir of hold-delay samples (first suppress to resolution), in microseconds.
static HOLD_DELAY: OnceLock<Mutex<PercentileTracker>> = OnceLock::new();
/// Start instant of the current hold window, if one is in progress.
static HOLD_START: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
/// Physical held-key set used to detect auto-repeat downs (low-level hooks have
/// no repeat bit). Only ever touched from the hook thread.
static HELD_KEYS: OnceLock<Mutex<BTreeSet<Key>>> = OnceLock::new();

/// The single explicit recording session. It observes one physical down without
/// modifying matcher state or suppressing input.
static CAPTURE_STATE: OnceLock<Mutex<Option<CaptureState>>> = OnceLock::new();
static CAPTURE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);

enum ControlAction {
    Suspend,
    Resume,
    ReplaceRules {
        index: inputflow_engine::RuleIndex,
        emergency_key: Key,
        rule_count: usize,
    },
    BeginCapture {
        timeout: Duration,
    },
    CancelCapture {
        session_id: u64,
    },
}

enum ControlResult {
    Suspended(PauseReport),
    Resumed,
    Replaced(ReplaceReport),
    CaptureStarted(CaptureSession),
    CaptureCancelled,
}

enum ControlRequestOutcome {
    Completed(Result<ControlResult, String>),
    Cancelled(ControlFailure),
    Failed(ControlFailure),
    OutcomeUnknown {
        request_id: u64,
        response: Receiver<Result<ControlResult, String>>,
    },
}

struct ControlRequest {
    id: u64,
    action: ControlAction,
    state: Arc<AtomicU8>,
    response: SyncSender<Result<ControlResult, String>>,
}

/// Auditable result of a completed pause transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PauseReport {
    pub held_events: usize,
    pub requested_inputs: u32,
    pub inserted_inputs: u32,
    pub output_complete: bool,
    pub last_error: u32,
}

/// Result of replacing the live rules at one Hook-owner serialization point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceReport {
    pub pause: PauseReport,
    pub was_suspended: bool,
    pub rule_count: usize,
    pub emergency_key: Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFailureKind {
    Cancelled,
    Failed,
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFailure {
    pub kind: ControlFailureKind,
    pub request_id: Option<u64>,
    pub message: String,
}

pub enum ReplaceRulesOutcome {
    Applied(ReplaceReport),
    Cancelled(ControlFailure),
    Failed(ControlFailure),
    OutcomeUnknown(PendingReplace),
}

pub struct PendingReplace {
    request_id: u64,
    response: Receiver<Result<ControlResult, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingReplaceOutcome {
    Applied(ReplaceReport),
    Failed(String),
    OutcomeUnknown(String),
}

impl PendingReplace {
    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    /// Wait for a rule replacement that had already started when its original
    /// bounded acknowledgement expired. Runtime owns this wait off the Hook
    /// thread and reconciles persisted/current configuration from the result.
    pub fn wait(self) -> PendingReplaceOutcome {
        match self.response.recv() {
            Ok(Ok(ControlResult::Replaced(report))) => PendingReplaceOutcome::Applied(report),
            Ok(Ok(_)) => PendingReplaceOutcome::Failed(
                "unexpected late control acknowledgement for rule replacement".to_string(),
            ),
            Ok(Err(error)) => PendingReplaceOutcome::Failed(error),
            Err(error) => PendingReplaceOutcome::OutcomeUnknown(format!(
                "late rule replacement result channel disconnected: {error}"
            )),
        }
    }
}

/// Stable recording payload forwarded to the future IPC layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturedInput {
    Key { logical: Key, physical: Option<Key> },
    MouseButton(MouseButton),
}

/// Terminal result for one bounded capture session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureOutcome {
    Captured(CapturedInput),
    Cancelled,
    TimedOut,
    Shutdown,
}

/// Receiver owned by the caller that began a capture session.
pub struct CaptureSession {
    id: u64,
    receiver: Receiver<CaptureOutcome>,
}

impl CaptureSession {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<CaptureOutcome, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}

struct CaptureState {
    id: u64,
    deadline: Instant,
    response: SyncSender<CaptureOutcome>,
}

/// Install or replace the matcher before a hook thread starts. This process-wide
/// cell is only a stable callback bridge; the value behind it is restartable.
pub fn install_matcher(matcher: Matcher) -> Result<(), String> {
    if HOOK_THREAD_ID.load(Ordering::Acquire) != 0 {
        return Err("cannot install a matcher while the hook thread is running".to_string());
    }
    if let Some(cell) = MATCHER.get() {
        *cell
            .lock()
            .map_err(|_| "matcher lock is poisoned".to_string())? = matcher;
    } else {
        MATCHER
            .set(Mutex::new(matcher))
            .map_err(|_| "failed to initialize matcher bridge".to_string())?;
    }
    Ok(())
}

/// Install the diagnostic-log sender. Call before the hook thread.
pub fn install_log_sender(tx: SyncSender<String>) -> Result<(), String> {
    let cell = LOG_TX.get_or_init(|| Mutex::new(None));
    *cell
        .lock()
        .map_err(|_| "diagnostic sender lock is poisoned".to_string())? = Some(tx);
    Ok(())
}

/// Detach the current diagnostic sender after the hook and logger stop.
pub fn clear_log_sender() {
    if let Some(cell) = LOG_TX.get()
        && let Ok(mut sender) = cell.lock()
    {
        *sender = None;
    }
}

/// Install the logical emergency bypass key. Call before the hook thread starts
/// or through the serialized replace-rules control request.
pub fn install_emergency_key(key: Key) -> Result<(), String> {
    let Some(index) = Key::NAMED.iter().position(|candidate| *candidate == key) else {
        return Err("emergency key must be a named logical key".to_string());
    };
    EMERGENCY_KEY_INDEX.store(index as u32 + 1, Ordering::Release);
    Ok(())
}

/// Reset process-level counters and transient state before a new runtime start.
/// This is legal only while no hook thread is live.
pub fn reset_runtime_state() -> Result<(), String> {
    if HOOK_THREAD_ID.load(Ordering::Acquire) != 0 {
        return Err("cannot reset state while the hook thread is running".to_string());
    }
    set_bypassed(false);
    SHUTTING_DOWN.store(false, Ordering::Relaxed);
    SEQ.store(0, Ordering::Relaxed);
    OUTPUT_SENT.store(0, Ordering::Relaxed);
    OUTPUT_FAILED.store(0, Ordering::Relaxed);
    OUTPUT_DROPPED.store(0, Ordering::Relaxed);
    CAPTURE_ACTIVE.store(false, Ordering::Relaxed);
    if let Ok(mut requests) = control_requests().lock() {
        requests.clear();
    }
    if let Ok(mut held) = held_keys().lock() {
        held.clear();
    }
    if let Some(cell) = CAPTURE_STATE.get()
        && let Ok(mut capture) = cell.lock()
    {
        *capture = None;
    }
    if let Some(tracker) = CALLBACK_LATENCY.get()
        && let Ok(mut tracker) = tracker.lock()
    {
        *tracker = PercentileTracker::new(100_000);
    }
    if let Some(tracker) = HOLD_DELAY.get()
        && let Ok(mut tracker) = tracker.lock()
    {
        *tracker = PercentileTracker::new(100_000);
    }
    if let Some(start) = HOLD_START.get()
        && let Ok(mut start) = start.lock()
    {
        *start = None;
    }
    Ok(())
}

/// Enable or disable per-event debug logging (off by default).
pub fn set_debug_log(enabled: bool) {
    DEBUG_LOG.store(enabled, Ordering::Relaxed);
}

/// Whether interception is currently stopped (bypass is active).
pub fn is_bypassed() -> bool {
    BYPASS.load(Ordering::Relaxed)
}

fn set_bypassed(bypassed: bool) {
    if BYPASS.swap(bypassed, Ordering::AcqRel) != bypassed {
        STATE_REVISION.fetch_add(1, Ordering::AcqRel);
        super::shell::notify_runtime_state_changed();
    }
}

/// Revision of the authoritative suspended state. Future status transports can
/// use this to coalesce notifications without creating a second state source.
pub fn state_revision() -> u64 {
    STATE_REVISION.load(Ordering::Acquire)
}

/// The configured emergency bypass key (default `F12`).
fn emergency_key() -> Key {
    let encoded = EMERGENCY_KEY_INDEX.load(Ordering::Acquire);
    if encoded == 0 {
        Key::F12
    } else {
        Key::NAMED
            .get(encoded as usize - 1)
            .copied()
            .unwrap_or(Key::F12)
    }
}

/// Whether per-event debug logging is enabled.
fn debug_log_enabled() -> bool {
    DEBUG_LOG.load(Ordering::Relaxed)
}

fn held_keys() -> &'static Mutex<BTreeSet<Key>> {
    HELD_KEYS.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// Update the physical held-key set for a keyboard event and report whether a
/// key-down is an auto-repeat (the key was already held). A key-up removes the
/// key and is never a repeat. On a poisoned lock we fail safe (non-repeat).
fn track_held_key(key: Key, up: bool) -> bool {
    let Ok(mut held) = held_keys().lock() else {
        return false;
    };
    if up {
        held.remove(&key);
        false
    } else {
        let repeat = held.contains(&key);
        held.insert(key);
        repeat
    }
}

fn control_requests() -> &'static Mutex<VecDeque<ControlRequest>> {
    CONTROL_REQUESTS.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn capture_state() -> &'static Mutex<Option<CaptureState>> {
    CAPTURE_STATE.get_or_init(|| Mutex::new(None))
}

fn finish_capture(outcome: CaptureOutcome) {
    let state = capture_state()
        .lock()
        .ok()
        .and_then(|mut state| state.take());
    CAPTURE_ACTIVE.store(false, Ordering::Release);
    if let Some(state) = state {
        let _ = state.response.try_send(outcome);
    }
}

fn observe_capture(event: InputEvent) {
    if event.injected || event.is_repeat() || event.is_key_up() || event.is_button_up() {
        return;
    }
    let captured = match event.source {
        InputSource::Keyboard { key, .. } if key != emergency_key() => CapturedInput::Key {
            logical: key,
            physical: event.physical_key(),
        },
        InputSource::Keyboard { .. } => return,
        InputSource::Mouse {
            kind: MouseKind::ButtonDown(button),
            ..
        } => CapturedInput::MouseButton(button),
        InputSource::Mouse { .. } => return,
    };
    finish_capture(CaptureOutcome::Captured(captured));
}

fn expire_capture() {
    let expired = capture_state()
        .lock()
        .ok()
        .and_then(|state| {
            state
                .as_ref()
                .map(|capture| Instant::now() >= capture.deadline)
        })
        .unwrap_or(false);
    if expired {
        finish_capture(CaptureOutcome::TimedOut);
    }
}

/// Pause the matcher and synchronously deliver its held events. This function
/// must run on the hook thread, so no physical callback can be processed between
/// taking the held events and completing their `SendInput` delivery.
fn flush_held_events_on_hook_thread_with(
    matcher: Option<&Mutex<Matcher>>,
    dispatch: impl FnOnce(&Command) -> OutputReport,
) -> PauseReport {
    let held = match matcher {
        Some(matcher) => match matcher.lock() {
            Ok(mut guard) => guard.set_paused(true),
            Err(_) => Vec::new(),
        },
        None => Vec::new(),
    };
    let held_events = held.len();
    let report = if held.is_empty() {
        OutputReport {
            requested: 0,
            inserted: 0,
            last_error: 0,
            status: OutputStatus::Complete,
        }
    } else {
        dispatch(&Command::Replay { events: held })
    };
    PauseReport {
        held_events,
        requested_inputs: report.requested,
        inserted_inputs: report.inserted,
        output_complete: report.status == OutputStatus::Complete,
        last_error: report.last_error,
    }
}

fn flush_held_events_on_hook_thread() -> PauseReport {
    flush_held_events_on_hook_thread_with(MATCHER.get(), dispatch_command)
}

fn matcher_has_release_tombstones() -> bool {
    MATCHER
        .get()
        .and_then(|matcher| matcher.lock().ok())
        .is_some_and(|guard| guard.has_release_tombstones())
}

fn suspend_on_hook_thread() -> PauseReport {
    let report = flush_held_events_on_hook_thread();
    set_bypassed(true);
    send_log(format!(
        "suspended: interception stopped; held_events={} inserted={} requested={} output_complete={} last_error={}",
        report.held_events,
        report.inserted_inputs,
        report.requested_inputs,
        report.output_complete,
        report.last_error
    ));
    report
}

fn resume_on_hook_thread() {
    if let Some(matcher) = MATCHER.get()
        && let Ok(mut guard) = matcher.lock()
    {
        guard.set_paused(false);
        guard.set_bypassed(false);
    }
    set_bypassed(false);
    send_log("resumed: interception active".to_string());
}

fn replace_rules_on_hook_thread(
    index: inputflow_engine::RuleIndex,
    emergency_key: Key,
    rule_count: usize,
) -> Result<ReplaceReport, String> {
    let was_suspended = is_bypassed();
    let pause = flush_held_events_on_hook_thread();
    set_bypassed(true);
    if !pause.output_complete {
        return Err(format!(
            "cannot replace rules because pending replay was incomplete (inserted={} requested={} last_error={})",
            pause.inserted_inputs, pause.requested_inputs, pause.last_error
        ));
    }

    let matcher = MATCHER
        .get()
        .ok_or_else(|| "matcher is not installed".to_string())?;
    let mut matcher = matcher
        .lock()
        .map_err(|_| "matcher lock is poisoned".to_string())?;
    matcher
        .replace_rules(index)
        .map_err(|error| error.to_string())?;
    install_emergency_key(emergency_key)?;
    if !was_suspended {
        matcher.set_paused(false);
        matcher.set_bypassed(false);
        set_bypassed(false);
    }
    drop(matcher);
    send_log(format!(
        "rules replaced: rule_count={rule_count} emergency_key={emergency_key} suspended={was_suspended}"
    ));
    Ok(ReplaceReport {
        pause,
        was_suspended,
        rule_count,
        emergency_key,
    })
}

fn begin_capture_on_hook_thread(timeout: Duration) -> Result<CaptureSession, String> {
    if timeout.is_zero() {
        return Err("capture timeout must be greater than zero".to_string());
    }
    let mut state = capture_state()
        .lock()
        .map_err(|_| "capture state lock is poisoned".to_string())?;
    if state.is_some() {
        return Err("a capture session is already active".to_string());
    }
    let id = CAPTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let (response, receiver) = sync_channel(1);
    *state = Some(CaptureState {
        id,
        deadline: Instant::now() + timeout,
        response,
    });
    CAPTURE_ACTIVE.store(true, Ordering::Release);
    send_log(format!("capture started: session_id={id}"));
    Ok(CaptureSession { id, receiver })
}

fn cancel_capture_on_hook_thread(session_id: u64) -> Result<(), String> {
    let active_id = capture_state()
        .lock()
        .map_err(|_| "capture state lock is poisoned".to_string())?
        .as_ref()
        .map(|capture| capture.id);
    match active_id {
        Some(id) if id == session_id => {
            finish_capture(CaptureOutcome::Cancelled);
            send_log(format!("capture cancelled: session_id={session_id}"));
            Ok(())
        }
        Some(id) => Err(format!(
            "capture session {session_id} is not active (active session is {id})"
        )),
        None => Err("no capture session is active".to_string()),
    }
}

fn execute_control(action: ControlAction) -> Result<ControlResult, String> {
    match action {
        ControlAction::Suspend => Ok(ControlResult::Suspended(suspend_on_hook_thread())),
        ControlAction::Resume => {
            resume_on_hook_thread();
            Ok(ControlResult::Resumed)
        }
        ControlAction::ReplaceRules {
            index,
            emergency_key,
            rule_count,
        } => replace_rules_on_hook_thread(index, emergency_key, rule_count)
            .map(ControlResult::Replaced),
        ControlAction::BeginCapture { timeout } => {
            begin_capture_on_hook_thread(timeout).map(ControlResult::CaptureStarted)
        }
        ControlAction::CancelCapture { session_id } => {
            cancel_capture_on_hook_thread(session_id)?;
            Ok(ControlResult::CaptureCancelled)
        }
    }
}

fn control_failure(
    kind: ControlFailureKind,
    request_id: Option<u64>,
    message: impl Into<String>,
) -> ControlFailure {
    ControlFailure {
        kind,
        request_id,
        message: message.into(),
    }
}

fn wait_for_control_result(
    request_id: u64,
    state: Arc<AtomicU8>,
    response: Receiver<Result<ControlResult, String>>,
    timeout: Duration,
    timeout_message: &str,
) -> ControlRequestOutcome {
    match response.recv_timeout(timeout) {
        Ok(result) => ControlRequestOutcome::Completed(result),
        Err(RecvTimeoutError::Disconnected) => {
            let current = state.load(Ordering::Acquire);
            if matches!(current, CONTROL_PENDING | CONTROL_CANCELLED) {
                ControlRequestOutcome::Failed(control_failure(
                    ControlFailureKind::Failed,
                    Some(request_id),
                    "hook thread ended before starting the control request",
                ))
            } else {
                ControlRequestOutcome::OutcomeUnknown {
                    request_id,
                    response,
                }
            }
        }
        Err(RecvTimeoutError::Timeout) => {
            if state
                .compare_exchange(
                    CONTROL_PENDING,
                    CONTROL_CANCELLED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                ControlRequestOutcome::Cancelled(control_failure(
                    ControlFailureKind::Cancelled,
                    Some(request_id),
                    format!(
                        "hook thread did not start control request within {CONTROL_ACK_TIMEOUT_MS}ms; request cancelled"
                    ),
                ))
            } else {
                ControlRequestOutcome::OutcomeUnknown {
                    request_id,
                    response,
                }
            }
        }
    }
    .with_timeout_context(timeout_message)
}

impl ControlRequestOutcome {
    fn with_timeout_context(self, context: &str) -> Self {
        match self {
            Self::OutcomeUnknown {
                request_id,
                response,
            } => {
                send_log(format!(
                    "control_outcome_unknown: request_id={request_id} context={context}"
                ));
                Self::OutcomeUnknown {
                    request_id,
                    response,
                }
            }
            outcome => outcome,
        }
    }
}

fn request_control(action: ControlAction) -> ControlRequestOutcome {
    let hook_thread_id = HOOK_THREAD_ID.load(Ordering::Acquire);
    if hook_thread_id == 0 {
        return ControlRequestOutcome::Failed(control_failure(
            ControlFailureKind::Failed,
            None,
            "hook thread is not ready",
        ));
    }
    // Emergency F12 already runs inside a hook callback. Execute directly to
    // avoid posting to and waiting on the current thread.
    let current_thread_id = unsafe { GetCurrentThreadId() };
    if current_thread_id == hook_thread_id {
        return ControlRequestOutcome::Completed(execute_control(action));
    }

    let id = CONTROL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let state = Arc::new(AtomicU8::new(CONTROL_PENDING));
    let (response_tx, response_rx) = sync_channel(1);
    let mut requests = match control_requests().lock() {
        Ok(requests) => requests,
        Err(_) => {
            return ControlRequestOutcome::Failed(control_failure(
                ControlFailureKind::Failed,
                Some(id),
                "control queue lock is poisoned",
            ));
        }
    };
    requests.push_back(ControlRequest {
        id,
        action,
        state: Arc::clone(&state),
        response: response_tx,
    });
    drop(requests);

    // SAFETY: the hook thread created its message queue before publishing its
    // id. The request itself is owned by the process-global queue.
    if unsafe { PostThreadMessageW(hook_thread_id, WM_INPUTFLOW_CONTROL, 0, 0) } == 0 {
        // Capture immediately; locking the local queue may overwrite it.
        let last_error = unsafe { GetLastError() };
        let removed = control_requests()
            .lock()
            .ok()
            .and_then(|mut requests| {
                let index = requests.iter().position(|request| request.id == id)?;
                requests.remove(index)
            })
            .is_some();
        if removed {
            return ControlRequestOutcome::Cancelled(control_failure(
                ControlFailureKind::Cancelled,
                Some(id),
                format!(
                    "failed to wake hook thread; request removed before execution (last_error={last_error})"
                ),
            ));
        }
        // Another control wake may already have dequeued this request. In that
        // case its acknowledgement, rather than the failed redundant wake,
        // determines the result.
        return wait_for_control_result(
            id,
            state,
            response_rx,
            Duration::from_millis(CONTROL_ACK_TIMEOUT_MS),
            &format!("wake_failed_last_error_{last_error}"),
        );
    }

    wait_for_control_result(
        id,
        state,
        response_rx,
        Duration::from_millis(CONTROL_ACK_TIMEOUT_MS),
        "ack_timeout",
    )
}

fn handle_control_requests() {
    loop {
        let request = match control_requests().lock() {
            Ok(mut requests) => requests.pop_front(),
            Err(_) => return,
        };
        let Some(request) = request else {
            return;
        };
        if request
            .state
            .compare_exchange(
                CONTROL_PENDING,
                CONTROL_RUNNING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            let _ = request.response.try_send(Err(
                "control request was cancelled before execution".to_string(),
            ));
            continue;
        }
        let result = execute_control(request.action);
        request.state.store(CONTROL_COMPLETE, Ordering::Release);
        let _ = request.response.try_send(result);
    }
}

fn fail_pending_control_requests(reason: &str) {
    let Ok(mut requests) = control_requests().lock() else {
        return;
    };
    for request in requests.drain(..) {
        request.state.store(CONTROL_CANCELLED, Ordering::Release);
        let _ = request.response.try_send(Err(reason.to_string()));
    }
}

fn completed_control(outcome: ControlRequestOutcome) -> Result<ControlResult, String> {
    match outcome {
        ControlRequestOutcome::Completed(result) => result,
        ControlRequestOutcome::Cancelled(failure) | ControlRequestOutcome::Failed(failure) => {
            Err(failure.message)
        }
        ControlRequestOutcome::OutcomeUnknown { request_id, .. } => Err(format!(
            "control request {request_id} started but its final outcome is unknown"
        )),
    }
}

/// Suspend interception on the hook thread and wait for replay delivery.
pub fn suspend() -> Result<PauseReport, String> {
    match completed_control(request_control(ControlAction::Suspend))? {
        ControlResult::Suspended(report) => Ok(report),
        _ => Err("unexpected control acknowledgement for suspend".to_string()),
    }
}

/// Resume interception on the hook thread, clearing any overflow bypass too.
pub fn resume() -> Result<(), String> {
    match completed_control(request_control(ControlAction::Resume))? {
        ControlResult::Resumed => Ok(()),
        _ => Err("unexpected control acknowledgement for resume".to_string()),
    }
}

/// Flush pending input and replace rules/emergency key on the Hook owner.
pub fn replace_rules(
    index: inputflow_engine::RuleIndex,
    emergency_key: Key,
    rule_count: usize,
) -> ReplaceRulesOutcome {
    match request_control(ControlAction::ReplaceRules {
        index,
        emergency_key,
        rule_count,
    }) {
        ControlRequestOutcome::Completed(Ok(ControlResult::Replaced(report))) => {
            ReplaceRulesOutcome::Applied(report)
        }
        ControlRequestOutcome::Completed(Ok(_)) => ReplaceRulesOutcome::Failed(control_failure(
            ControlFailureKind::Failed,
            None,
            "unexpected control acknowledgement for rule replacement",
        )),
        ControlRequestOutcome::Completed(Err(error)) => {
            ReplaceRulesOutcome::Failed(control_failure(ControlFailureKind::Failed, None, error))
        }
        ControlRequestOutcome::Cancelled(failure) => ReplaceRulesOutcome::Cancelled(failure),
        ControlRequestOutcome::Failed(failure) => ReplaceRulesOutcome::Failed(failure),
        ControlRequestOutcome::OutcomeUnknown {
            request_id,
            response,
        } => ReplaceRulesOutcome::OutcomeUnknown(PendingReplace {
            request_id,
            response,
        }),
    }
}

/// Begin a bounded recording session without changing interception state.
pub fn begin_capture(timeout: Duration) -> Result<CaptureSession, String> {
    match completed_control(request_control(ControlAction::BeginCapture { timeout }))? {
        ControlResult::CaptureStarted(session) => Ok(session),
        _ => Err("unexpected control acknowledgement for capture start".to_string()),
    }
}

/// Cancel the named capture session on the Hook owner.
pub fn cancel_capture(session_id: u64) -> Result<(), String> {
    match completed_control(request_control(ControlAction::CancelCapture { session_id }))? {
        ControlResult::CaptureCancelled => Ok(()),
        _ => Err("unexpected control acknowledgement for capture cancel".to_string()),
    }
}

pub fn is_capture_active() -> bool {
    CAPTURE_ACTIVE.load(Ordering::Acquire)
}

/// Toggle suspension and return the new state (`true` = interception stopped).
pub fn toggle_suspend() -> Result<bool, String> {
    if is_bypassed() {
        resume()?;
        Ok(false)
    } else {
        suspend()?;
        Ok(true)
    }
}

/// Whether interception is currently suspended (same flag as bypass).
pub fn is_suspended() -> bool {
    is_bypassed()
}

/// Flush any held events during clean shutdown. The caller is the hook thread.
fn flush_held() {
    let _ = flush_held_events_on_hook_thread();
}

fn callback_latency_tracker() -> &'static Mutex<PercentileTracker> {
    CALLBACK_LATENCY.get_or_init(|| Mutex::new(PercentileTracker::new(100_000)))
}

fn hold_delay_tracker() -> &'static Mutex<PercentileTracker> {
    HOLD_DELAY.get_or_init(|| Mutex::new(PercentileTracker::new(100_000)))
}

fn hold_start_cell() -> &'static Mutex<Option<Instant>> {
    HOLD_START.get_or_init(|| Mutex::new(None))
}

fn record_callback_latency(start: Instant) {
    if let Ok(mut tracker) = callback_latency_tracker().lock() {
        // Take elapsed only after acquiring the recorder lock so contention with
        // a concurrent stats snapshot is represented in this sample.
        tracker.record(start.elapsed().as_micros() as u64);
    }
}

fn finish_callback(start: Instant, result: LRESULT) -> LRESULT {
    record_callback_latency(start);
    result
}

fn record_hold_delay() {
    let start = match hold_start_cell().lock() {
        Ok(mut cell) => cell.take(),
        Err(_) => None,
    };
    if let Some(start) = start
        && let Ok(mut tracker) = hold_delay_tracker().lock()
    {
        tracker.record(start.elapsed().as_micros() as u64);
    }
}

/// A latency/delay stats summary: `(total, p50, p95, p99, max)` in microseconds.
pub type Percentiles = (u64, Option<u64>, Option<u64>, Option<u64>, Option<u64>);

fn tracker_summary_with(
    tracker: &Mutex<PercentileTracker>,
    summarize: impl FnOnce(PercentileSnapshot) -> PercentileSummary,
) -> Option<PercentileSummary> {
    // Copy at most 100,000 u64 values while locked, then perform the O(n log n)
    // sort after releasing the hot-path recorder mutex.
    let snapshot = tracker.lock().ok()?.snapshot();
    Some(summarize(snapshot))
}

fn tracker_stats(tracker: &Mutex<PercentileTracker>) -> Option<Percentiles> {
    let summary = tracker_summary_with(tracker, PercentileSnapshot::summary)?;
    Some((
        summary.total,
        summary.p50,
        summary.p95,
        summary.p99,
        summary.max,
    ))
}

/// Observed callback duration through acquisition of the stats recorder. It
/// includes normalization, matcher-lock wait, matching/output, logging enqueue,
/// `CallNextHookEx` for forwarded events, and recorder-lock wait. It necessarily
/// excludes the final sample write/unlock performed after elapsed is read.
pub fn callback_latency_stats() -> Option<Percentiles> {
    tracker_stats(callback_latency_tracker())
}

/// (total, p50, p95, p99, max) for hold delay in microseconds, if sampled.
pub fn hold_delay_stats() -> Option<Percentiles> {
    tracker_stats(hold_delay_tracker())
}

/// Number of events that were normalized (assigned a sequence number).
pub fn seq_count() -> u64 {
    SEQ.load(Ordering::Relaxed)
}

/// Number of successful output batches.
pub fn output_sent() -> u64 {
    OUTPUT_SENT.load(Ordering::Relaxed)
}

/// Number of failed output batches.
pub fn output_failed() -> u64 {
    OUTPUT_FAILED.load(Ordering::Relaxed)
}

/// Legacy dropped-output counter. Synchronous output has no command queue, so
/// current code never increments this value.
pub fn output_dropped() -> u64 {
    OUTPUT_DROPPED.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputStatus {
    Complete,
    ZeroInserted,
    PartiallyInserted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutputReport {
    requested: u32,
    inserted: u32,
    last_error: u32,
    status: OutputStatus,
}

#[derive(Debug, Clone, Copy)]
struct SendCall {
    inserted: u32,
    last_error: u32,
}

fn attempt_output_with(
    command: &Command,
    mut sender: impl FnMut(&[INPUT]) -> SendCall,
) -> OutputReport {
    let inputs = build_inputs(command);
    let requested = inputs.len() as u32;
    let call = sender(&inputs);
    let status = if call.inserted == requested {
        OutputStatus::Complete
    } else if call.inserted == 0 {
        OutputStatus::ZeroInserted
    } else {
        OutputStatus::PartiallyInserted
    };
    OutputReport {
        requested,
        inserted: call.inserted,
        last_error: call.last_error,
        status,
    }
}

fn execute_report(command: &Command) -> OutputReport {
    let report = attempt_output_with(command, |inputs| {
        // SAFETY: `inputs` is fully initialized; `pinputs` points to `len` valid
        // elements; `cbsize` is `size_of::<INPUT>()`.
        let inserted = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                core::mem::size_of::<INPUT>() as i32,
            )
        };
        // GetLastError cannot identify UIPI, but capture it immediately so the
        // diagnostic is still useful for other failures.
        let last_error = if inserted == inputs.len() as u32 {
            0
        } else {
            unsafe { GetLastError() }
        };
        SendCall {
            inserted,
            last_error,
        }
    });
    if report.status == OutputStatus::Complete {
        OUTPUT_SENT.fetch_add(1, Ordering::Relaxed);
    } else {
        OUTPUT_FAILED.fetch_add(1, Ordering::Relaxed);
        let kind = match report.status {
            OutputStatus::ZeroInserted => "zero_inserted",
            OutputStatus::PartiallyInserted => "partial_insert",
            OutputStatus::Complete => unreachable!(),
        };
        send_log(format!(
            "sendinput_failed: kind={kind} inserted={} requested={} last_error={}; entering bypass; prior suppressed input is not guaranteed recovered",
            report.inserted, report.requested, report.last_error
        ));
        // This protects only future input. Previously suppressed events may
        // already be unrecoverable (especially after a partial insertion).
        set_bypassed(true);
    }
    report
}

/// Execute one output command via `SendInput` and return the inserted count.
pub fn execute(command: &Command) -> u32 {
    execute_report(command).inserted
}

fn is_own_event(extra_info: usize) -> bool {
    extra_info == SELF_EXTRA_INFO_TAG
}

fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

fn send_log(line: String) {
    if let Some(cell) = LOG_TX.get()
        && let Ok(sender) = cell.lock()
        && let Some(sender) = sender.as_ref()
    {
        let _ = sender.try_send(line);
    }
}

/// Feed one normalized event to the matcher and return the decision. Any
/// resulting replay / action command is executed synchronously via `SendInput`.
/// The enclosing hook callback records full wall time.
fn process(event: InputEvent) -> Decision {
    let resolution_start = Instant::now();
    let Some(matcher) = MATCHER.get() else {
        return Decision::PassThrough;
    };
    let Ok(mut guard) = matcher.lock() else {
        // Poisoned lock: fail safe by passing through.
        return Decision::PassThrough;
    };
    // BYPASS normally accompanies matcher pause/bypass, but output failure sets
    // the atomic first. Synchronize here before handling the next event. The
    // matcher still consumes release tombstones before applying pause.
    if is_bypassed() {
        guard.set_paused(true);
    }
    let (mut decision, resolution) = guard.on_event(event);

    // A suppressed down starts (or continues) a hold-delay window.
    if matches!(decision, Decision::Suppress { .. })
        && matches!(resolution, Resolution::Pending)
        && (event.is_key_down() || event.is_button_down())
        && let Ok(mut cell) = hold_start_cell().lock()
        && cell.is_none()
    {
        *cell = Some(resolution_start);
    }

    // A queue overflow inside the matcher also stops interception at the
    // platform level so the hook callbacks short-circuit process().
    if guard.is_bypassed() {
        set_bypassed(true);
        send_log("queue_overflow: entering bypass".to_string());
    }

    let command = match resolution {
        Resolution::Matched { rule_id, action } => Some(Command::Emit { rule_id, action }),
        Resolution::Failed { replay } => Some(Command::Replay { events: replay }),
        Resolution::Pending => None,
    };
    if let Some(command) = command {
        let report = dispatch_command(&command);
        if report.status != OutputStatus::Complete {
            // Enter matcher bypass atomically with the failed decision and keep
            // consumed release tombstones. Any still-pending events cannot be
            // truthfully described as recovered, so only diagnose them.
            let stranded = guard.set_paused(true);
            guard.set_bypassed(true);
            if !stranded.is_empty() {
                send_log(format!(
                    "output_failure_stranded: {} additional held event(s) could not be replayed",
                    stranded.len()
                ));
            }
            // If replay inserted nothing and the current suppressed event is the
            // final replay item, the hook can still fail open for that one event
            // by forwarding it. Earlier held events remain unrecovered.
            let recovered_decision =
                decision_after_output_failure(decision, &command, event, report);
            if recovered_decision != decision {
                decision = recovered_decision;
                send_log(
                    "sendinput_zero: current event forwarded; earlier suppressed events may be lost"
                        .to_string(),
                );
            }
        }
    }
    decision
}

fn decision_after_output_failure(
    decision: Decision,
    command: &Command,
    current: InputEvent,
    report: OutputReport,
) -> Decision {
    if report.status == OutputStatus::ZeroInserted
        && matches!(decision, Decision::Suppress { .. })
        && matches!(command, Command::Replay { events } if events.last().is_some_and(|last| last.seq == current.seq))
    {
        Decision::PassThrough
    } else {
        // After partial insertion we cannot prove which individual event was
        // accepted merely from the aggregate count, so forwarding could create
        // a duplicate. Matched trigger input also remains consumed.
        decision
    }
}

/// Log and synchronously execute one output command via `SendInput`. Ends the
/// current hold-delay window so the hold total delay can be sampled. Running
/// `SendInput` here (rather than on a worker) guarantees the replay/action is
/// inserted before the hook returns, so a subsequent pass-through event cannot
/// overtake it (M6 P1 #4), and there is no queue to drop or block on (#5).
fn dispatch_command(command: &Command) -> OutputReport {
    match command {
        Command::Emit { rule_id, action } => {
            if debug_log_enabled() {
                send_log(format!("matched rule `{rule_id}` with {action:?}"));
            } else {
                send_log("matched rule; detailed identity disabled".to_string());
            }
        }
        Command::Replay { events } => {
            send_log(format!("replay {} held event(s)", events.len()));
        }
    }
    record_hold_delay();
    let report = execute_report(command);
    send_log(format!(
        "output: inserted={} requested={} status={:?}",
        report.inserted, report.requested, report.status
    ));
    report
}

/// Drive any elapsed matcher deadline (Hold / Hold+MouseButton). Called from the
/// message loop on `WM_TIMER`; runs on the hook thread but outside the callback.
fn drive_timeouts() {
    expire_capture();
    let Some(matcher) = MATCHER.get() else {
        return;
    };
    let Ok(mut guard) = matcher.lock() else {
        return;
    };
    let mut resolved = 0usize;
    for command in guard.poll_timeouts() {
        resolved += 1;
        let report = dispatch_command(&command);
        if report.status != OutputStatus::Complete {
            guard.set_paused(true);
            guard.set_bypassed(true);
            break;
        }
    }
    if resolved > 0 {
        send_log(format!("timeout: {resolved} deadline(s) resolved"));
    }
}

/// Build the `INPUT` array for an output command.
fn build_inputs(command: &Command) -> Vec<INPUT> {
    match command {
        Command::Replay { events } => events.iter().filter_map(event_to_input).collect(),
        Command::Emit { action, .. } => match action {
            Action::KeyChord(keys) => {
                let mut inputs = Vec::with_capacity(keys.len() * 2);
                for &key in keys {
                    inputs.push(make_action_key_input(key, false));
                }
                for &key in keys.iter().rev() {
                    inputs.push(make_action_key_input(key, true));
                }
                inputs
            }
        },
    }
}

/// Convert one engine event into a Win32 `INPUT`, or `None` for wheel/move.
///
/// Note: mouse button events are replayed at the *current* cursor position; the
/// `x`/`y` captured at observation time are intentionally not restored (M6
/// round-2 F). This means a replayed click after the cursor moved lands at the
/// new position. Restoring absolute position is left as a documented limitation.
fn event_to_input(event: &InputEvent) -> Option<INPUT> {
    match event.source {
        InputSource::Keyboard {
            key,
            scan_code,
            extended,
            down,
            ..
        } => Some(make_replay_key_input(key, scan_code, extended, !down)),
        InputSource::Mouse {
            kind: MouseKind::ButtonDown(button),
            ..
        } => Some(make_mouse_input(button, true)),
        InputSource::Mouse {
            kind: MouseKind::ButtonUp(button),
            ..
        } => Some(make_mouse_input(button, false)),
        InputSource::Mouse { .. } => None,
    }
}

/// Build `KEYBDINPUT.dwFlags`. Captured-event replay and physical actions use
/// scan-code mode; logical actions use VK mode. Pure so it can be unit-tested.
fn keybd_flags(up: bool, extended: bool, scan_code_mode: bool) -> u32 {
    let mut flags = 0u32;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if scan_code_mode {
        flags |= KEYEVENTF_SCANCODE;
    }
    flags
}

/// Build one keyboard `INPUT` for a configured action identity.
fn make_action_key_input(key: Key, up: bool) -> INPUT {
    match key {
        Key::Physical {
            scan_code,
            extended,
        } => make_scan_key_input(scan_code, extended, up),
        _ => make_logical_key_input(key, up),
    }
}

/// Replay a captured event by scan code whenever possible. This preserves its
/// physical position across layout changes. Events without a scan code (some
/// media/vendor input) fall back to the observed logical VK.
fn make_replay_key_input(key: Key, scan_code: u16, extended: bool, up: bool) -> INPUT {
    if scan_code != 0 {
        make_scan_key_input(scan_code, extended, up)
    } else {
        make_logical_key_input(key, up)
    }
}

fn make_logical_key_input(key: Key, up: bool) -> INPUT {
    let vk = keymap::key_to_vk(key).expect("logical action/replay key must have a VK");
    make_key_input(vk, 0, keymap::key_is_extended(key), up, false)
}

fn make_scan_key_input(scan_code: u16, extended: bool, up: bool) -> INPUT {
    debug_assert_ne!(scan_code, 0);
    make_key_input(0, scan_code, extended, up, true)
}

/// Build one keyboard `INPUT` from already-selected VK/scan-code fields.
// `INPUT` holds a union (`Anonymous`), so `Default` + field assignment is the
// clearest way to set it; the lint is a false positive here.
#[allow(clippy::field_reassign_with_default)]
fn make_key_input(
    vk: u16,
    scan_code: u16,
    extended: bool,
    up: bool,
    scan_code_mode: bool,
) -> INPUT {
    let mut input = INPUT::default();
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki = KEYBDINPUT {
        wVk: vk,
        wScan: scan_code,
        dwFlags: keybd_flags(up, extended, scan_code_mode),
        time: 0,
        dwExtraInfo: SELF_EXTRA_INFO_TAG,
    };
    input
}

/// Build one mouse-button `INPUT`.
// Same `Default` + union-field-assignment rationale as `make_key_input`.
#[allow(clippy::field_reassign_with_default)]
fn make_mouse_input(button: MouseButton, down: bool) -> INPUT {
    let (flags, mouse_data) = match (button, down) {
        (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
        (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
        (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
        (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
        (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
        (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
        (MouseButton::XButton1, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
        (MouseButton::XButton1, false) => (MOUSEEVENTF_XUP, XBUTTON1),
        (MouseButton::XButton2, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
        (MouseButton::XButton2, false) => (MOUSEEVENTF_XUP, XBUTTON2),
    };
    let mut input = INPUT::default();
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi = MOUSEINPUT {
        dx: 0,
        dy: 0,
        mouseData: mouse_data,
        dwFlags: flags,
        time: 0,
        dwExtraInfo: SELF_EXTRA_INFO_TAG,
    };
    input
}

/// Post `WM_QUIT` to the hook thread. Returns `false` if the call failed.
pub fn post_quit(hook_thread_id: u32) -> bool {
    // SAFETY: `hook_thread_id` belongs to the hook thread, whose message queue
    // was created (via PeekMessageW) before it reported the id.
    let ret = unsafe { PostThreadMessageW(hook_thread_id, WM_QUIT, 0, 0) };
    ret != 0
}

/// Entry point for the dedicated hook thread.
pub fn run_hook_thread(ready: std::sync::mpsc::Sender<Result<u32, String>>) {
    // SAFETY: install_hooks documents its own preconditions; handles are
    // returned to be cleaned up below.
    let hooks = match unsafe { install_hooks() } {
        Ok(hooks) => hooks,
        Err(err) => {
            send_log(format!("hook install failed: {err}"));
            let _ = ready.send(Err(err));
            return;
        }
    };
    send_log("hooks installed (keyboard + mouse)".to_string());

    // Create this thread's message queue before signaling readiness.
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid pointer to a zeroed MSG; null hwnd queries this
    // thread's queue. PM_NOREMOVE creates the queue without consuming a message.
    unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };

    // Install a thread timer that posts WM_TIMER to this thread's queue, so the
    // message loop can drive Hold / Hold+MouseButton deadlines. A null hwnd with
    // no callback delivers WM_TIMER through the thread's message queue. Timer
    // failure is fatal: reporting ready without a deadline driver could hold a
    // timed prefix indefinitely.
    let timer_id = match install_timeout_timer_with(|| {
        // SAFETY: null hwnd associates the timer with this thread's message
        // queue (created above). ID 0 requests a generated thread-timer ID.
        let timer_id =
            unsafe { SetTimer(std::ptr::null_mut(), 0, TIMEOUT_TIMER_INTERVAL_MS, None) };
        let last_error = if timer_id == 0 {
            // SAFETY: no preconditions; captured immediately after SetTimer.
            unsafe { GetLastError() }
        } else {
            0
        };
        (timer_id, last_error)
    }) {
        Ok(timer_id) => timer_id,
        Err(error) => {
            send_log(error.clone());
            let _ = ready.send(Err(error));
            // SAFETY: hooks were successfully installed above.
            unsafe { uninstall_hooks(hooks) };
            return;
        }
    };

    // Readiness is reported only after hooks, queue, and deadline driver are all
    // live.
    // SAFETY: GetCurrentThreadId has no preconditions.
    let tid = unsafe { GetCurrentThreadId() };
    HOOK_THREAD_ID.store(tid, Ordering::Release);
    if ready.send(Ok(tid)).is_err() {
        HOOK_THREAD_ID.store(0, Ordering::Release);
        fail_pending_control_requests("hook thread stopped before becoming ready");
        // SAFETY: timer and hooks were successfully installed above.
        unsafe {
            KillTimer(std::ptr::null_mut(), timer_id);
            uninstall_hooks(hooks);
        }
        return;
    }

    // SAFETY: runs the message loop until GetMessageW returns <= 0.
    unsafe { run_message_loop() };
    HOOK_THREAD_ID.store(0, Ordering::Release);
    fail_pending_control_requests("hook thread is shutting down");

    // Best-effort flush any held events before removing the hooks.
    SHUTTING_DOWN.store(true, Ordering::Relaxed);
    finish_capture(CaptureOutcome::Shutdown);
    flush_held();
    set_bypassed(true);
    if matcher_has_release_tombstones() {
        send_log("shutdown_drain: waiting up to 2000ms for consumed input releases".to_string());
        // SAFETY: still on the hook/message-loop thread with hooks and timer
        // installed. The bounded loop only pumps messages until releases clear.
        unsafe { drain_release_tombstones() };
    }
    if matcher_has_release_tombstones() {
        send_log(
            "shutdown_limit: consumed input is still physically held after the drain timeout; releases after hook removal cannot be suppressed"
                .to_string(),
        );
    }

    // SAFETY: `timer_id` was created by this thread above.
    unsafe { KillTimer(std::ptr::null_mut(), timer_id) };

    // SAFETY: hooks are still owned by this thread and must be removed before exit.
    unsafe { uninstall_hooks(hooks) };
}

fn install_timeout_timer_with(install: impl FnOnce() -> (usize, u32)) -> Result<usize, String> {
    let (timer_id, last_error) = install();
    if timer_id == 0 {
        Err(format!(
            "timeout timer installation failed (last_error={last_error}); hooks disabled"
        ))
    } else {
        Ok(timer_id)
    }
}

/// Handles to the two installed low-level hooks.
struct HookHandles {
    keyboard: *mut core::ffi::c_void,
    mouse: *mut core::ffi::c_void,
}

/// Install `WH_KEYBOARD_LL` and `WH_MOUSE_LL`.
///
/// # Safety
/// Must be called from the thread that will run the message loop.
unsafe fn install_hooks() -> Result<HookHandles, String> {
    // SAFETY: a null module name returns the handle of the current executable.
    let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };

    // SAFETY: the hook procedures are defined in this module, `hmod` is the
    // current module handle, and `dwThreadId == 0` installs a global low-level
    // hook delivered on the calling thread (which runs a message loop).
    let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0) };
    if keyboard.is_null() {
        // SAFETY: no preconditions.
        let err = unsafe { GetLastError() };
        return Err(format!("SetWindowsHookExW(WH_KEYBOARD_LL) failed: {err}"));
    }

    // SAFETY: same contract as the keyboard hook above.
    let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0) };
    if mouse.is_null() {
        // SAFETY: `keyboard` is a valid, installed hook handle.
        unsafe { UnhookWindowsHookEx(keyboard) };
        // SAFETY: no preconditions.
        let err = unsafe { GetLastError() };
        return Err(format!("SetWindowsHookExW(WH_MOUSE_LL) failed: {err}"));
    }

    Ok(HookHandles { keyboard, mouse })
}

/// Uninstall both hooks.
///
/// # Safety
/// `hooks` must be valid, installed hook handles owned by the calling thread.
unsafe fn uninstall_hooks(hooks: HookHandles) {
    // SAFETY: both handles are valid and installed.
    unsafe {
        UnhookWindowsHookEx(hooks.mouse);
        UnhookWindowsHookEx(hooks.keyboard);
    }
}

/// Pump messages until `WM_QUIT` (or a `GetMessageW` error) arrives.
///
/// # Safety
/// Must be called on the thread that owns the message queue.
unsafe fn run_message_loop() {
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid pointer; null hwnd retrieves any message for
        // this thread.
        let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if ret <= 0 {
            break;
        }
        if msg.message == WM_TIMER {
            // Deadline tick: drive the matcher's Hold / Hold+MouseButton
            // deadlines. There is no window to translate/dispatch a timer to.
            drive_timeouts();
            continue;
        }
        if msg.message == WM_INPUTFLOW_CONTROL {
            handle_control_requests();
            continue;
        }
        // SAFETY: `msg` was just retrieved and is valid.
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Pump the hook thread for a bounded shutdown grace period so physical ups can
/// clear consumed release tombstones before hooks are removed.
///
/// # Safety
/// Must run on the thread that owns the installed hooks and message queue.
unsafe fn drain_release_tombstones() {
    let deadline = Instant::now() + std::time::Duration::from_millis(SHUTDOWN_TOMBSTONE_DRAIN_MS);
    let mut msg = MSG::default();
    while matcher_has_release_tombstones() && Instant::now() < deadline {
        // SAFETY: `msg` is valid and this function runs on the queue owner.
        let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if ret <= 0 {
            break;
        }
        if msg.message == WM_TIMER {
            continue;
        }
        // SAFETY: `msg` was just retrieved and is valid.
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// `WH_KEYBOARD_LL` callback: normalize, log, then decide suppression via the
/// matcher.
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let callback_start = Instant::now();
        // SAFETY: `code >= HC_ACTION` guarantees a valid KBDLLHOOKSTRUCT.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let (up, extended, injected) = keymap::keyboard_flags(info.flags);
        let scan_code = info.scanCode as u16;
        let key = keymap::hook_key(info.vkCode as u16, scan_code, extended);

        // Our own synthesized events pass through, so replay never recurses. Do
        // this before the held-key bookkeeping so replayed events do not affect
        // auto-repeat detection.
        if is_own_event(info.dwExtraInfo) {
            // SAFETY: `hhk` is ignored for low-level hooks.
            let result = unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
            return finish_callback(callback_start, result);
        }

        // Low-level hooks expose no repeat bit; derive auto-repeat from the
        // maintained physical held-key set (M6 P0 #2).
        let tracking_key = Key::physical(scan_code, extended).unwrap_or(key);
        let repeat = track_held_key(tracking_key, up);

        let event = InputEvent {
            seq: next_seq(),
            time_ms: info.time as u64,
            injected,
            source: InputSource::Keyboard {
                key,
                scan_code,
                extended,
                down: !up,
                repeat,
            },
        };
        if debug_log_enabled() {
            send_log(format!(
                "[seq={:06}] kbd {} logical={key} scan=0x{scan_code:02X} extended={extended} injected={injected} repeat={repeat}",
                event.seq,
                if up { "Up" } else { "Down" },
            ));
        }

        // Capture is an immutable observation only. It neither pauses nor
        // pre-consumes the event, and the emergency key is excluded inside the
        // observer.
        observe_capture(event);

        // The emergency key toggles suspension on its first (non-repeat) down and
        // is itself never intercepted.
        if !up && !repeat && key == emergency_key() && !SHUTTING_DOWN.load(Ordering::Relaxed) {
            match toggle_suspend() {
                Ok(suspended) => send_log(format!(
                    "suspended {}",
                    if suspended {
                        "ON (interception stopped)"
                    } else {
                        "OFF (interception active)"
                    }
                )),
                Err(error) => send_log(format!("suspend_toggle_failed: {error}")),
            }
            // SAFETY: `hhk` is ignored for low-level hooks.
            let result = unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
            return finish_callback(callback_start, result);
        }

        match process(event) {
            Decision::Suppress { .. } => finish_callback(callback_start, 1),
            Decision::PassThrough => {
                // SAFETY: `hhk` is ignored for low-level hooks.
                let result = unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
                finish_callback(callback_start, result)
            }
        }
    } else {
        // SAFETY: `hhk` is ignored for low-level hooks.
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }
}

/// `WH_MOUSE_LL` callback: normalize buttons, log, then decide via the matcher.
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let callback_start = Instant::now();
        // SAFETY: `code >= HC_ACTION` guarantees a valid MSLLHOOKSTRUCT.
        let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let injected = (info.flags & LLMHF_INJECTED) != 0;

        // Only button events participate in matching; wheel/move pass through.
        if let Some((button, down)) = keymap::mouse_wparam(wparam as u32, info.mouseData) {
            let event = InputEvent {
                seq: next_seq(),
                time_ms: info.time as u64,
                injected,
                source: InputSource::Mouse {
                    kind: if down {
                        MouseKind::ButtonDown(button)
                    } else {
                        MouseKind::ButtonUp(button)
                    },
                    x: info.pt.x,
                    y: info.pt.y,
                },
            };
            if debug_log_enabled() {
                send_log(format!(
                    "[seq={:06}] mouse {} {button:?} injected={injected}",
                    event.seq,
                    if down { "Down" } else { "Up" },
                ));
            }

            if is_own_event(info.dwExtraInfo) {
                // SAFETY: `hhk` is ignored for low-level hooks.
                let result = unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
                return finish_callback(callback_start, result);
            }

            observe_capture(event);

            match process(event) {
                Decision::Suppress { .. } => finish_callback(callback_start, 1),
                Decision::PassThrough => {
                    // SAFETY: `hhk` is ignored for low-level hooks.
                    let result =
                        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
                    finish_callback(callback_start, result)
                }
            }
        } else {
            // SAFETY: `hhk` is ignored for low-level hooks.
            let result = unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
            finish_callback(callback_start, result)
        }
    } else {
        // SAFETY: `hhk` is ignored for low-level hooks.
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    use inputflow_engine::{ManualClock, Rule, RuleIndex, Trigger};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse as kb;

    fn key_event(seq: u64, key: Key, down: bool) -> InputEvent {
        InputEvent {
            seq,
            time_ms: seq,
            injected: false,
            source: InputSource::Keyboard {
                key,
                scan_code: 0,
                extended: false,
                down,
                repeat: false,
            },
        }
    }

    #[test]
    fn keybd_flags_sets_up_and_extended_bits() {
        assert_eq!(keybd_flags(false, false, false), 0);
        assert_eq!(keybd_flags(true, false, false), KEYEVENTF_KEYUP);
        assert_eq!(keybd_flags(false, true, false), KEYEVENTF_EXTENDEDKEY);
        assert_eq!(
            keybd_flags(true, true, true),
            KEYEVENTF_KEYUP | KEYEVENTF_EXTENDEDKEY | KEYEVENTF_SCANCODE
        );
    }

    #[test]
    fn captured_replay_uses_scan_code_and_preserves_down_up_direction() {
        let down = InputEvent {
            seq: 0,
            time_ms: 0,
            injected: false,
            source: InputSource::Keyboard {
                key: Key::CapsLock,
                scan_code: 0x3a,
                extended: false,
                down: true,
                repeat: false,
            },
        };
        let up = InputEvent {
            seq: 1,
            time_ms: 1,
            injected: false,
            source: InputSource::Keyboard {
                key: Key::CapsLock,
                scan_code: 0x3a,
                extended: false,
                down: false,
                repeat: false,
            },
        };

        let down_input = event_to_input(&down).unwrap();
        let up_input = event_to_input(&up).unwrap();
        // SAFETY: both values were initialized above as INPUT_KEYBOARD.
        let down_ki = unsafe { down_input.Anonymous.ki };
        // SAFETY: both values were initialized above as INPUT_KEYBOARD.
        let up_ki = unsafe { up_input.Anonymous.ki };
        assert_eq!((down_ki.wVk, down_ki.wScan), (0, 0x3a));
        assert_eq!(down_ki.dwFlags, KEYEVENTF_SCANCODE);
        assert_eq!(up_ki.dwFlags, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP);
    }

    #[test]
    fn capture_observes_one_down_without_suppressing_or_recording_emergency() {
        reset_runtime_state().unwrap();
        install_emergency_key(Key::F12).unwrap();
        let session = begin_capture_on_hook_thread(Duration::from_secs(1)).unwrap();
        let emergency = InputEvent {
            seq: 0,
            time_ms: 0,
            injected: false,
            source: InputSource::Keyboard {
                key: Key::F12,
                scan_code: 0x58,
                extended: false,
                down: true,
                repeat: false,
            },
        };
        observe_capture(emergency);
        assert!(is_capture_active());

        let key = InputEvent {
            seq: 1,
            time_ms: 1,
            injected: false,
            source: InputSource::Keyboard {
                key: Key::Oem1,
                scan_code: 0x27,
                extended: false,
                down: true,
                repeat: false,
            },
        };
        observe_capture(key);
        assert_eq!(
            session.recv_timeout(Duration::from_millis(10)),
            Ok(CaptureOutcome::Captured(CapturedInput::Key {
                logical: Key::Oem1,
                physical: Some(Key::Physical {
                    scan_code: 0x27,
                    extended: false,
                }),
            }))
        );
        assert!(!is_capture_active());

        let cancelled = begin_capture_on_hook_thread(Duration::from_secs(1)).unwrap();
        cancel_capture_on_hook_thread(cancelled.id()).unwrap();
        assert_eq!(
            cancelled.recv_timeout(Duration::from_millis(10)),
            Ok(CaptureOutcome::Cancelled)
        );
    }

    #[test]
    fn running_control_timeout_preserves_late_replace_completion() {
        let state = Arc::new(AtomicU8::new(CONTROL_RUNNING));
        let (response_tx, response_rx) = sync_channel(1);
        let outcome = wait_for_control_result(
            77,
            Arc::clone(&state),
            response_rx,
            Duration::from_millis(1),
            "injected_delay",
        );
        let ControlRequestOutcome::OutcomeUnknown {
            request_id,
            response,
        } = outcome
        else {
            panic!("a running request must not be reported as cancelled or failed");
        };
        assert_eq!(request_id, 77);

        let report = ReplaceReport {
            pause: PauseReport {
                held_events: 0,
                requested_inputs: 0,
                inserted_inputs: 0,
                output_complete: true,
                last_error: 0,
            },
            was_suspended: false,
            rule_count: 1,
            emergency_key: Key::F12,
        };
        state.store(CONTROL_COMPLETE, Ordering::Release);
        response_tx
            .send(Ok(ControlResult::Replaced(report)))
            .unwrap();
        assert_eq!(
            PendingReplace {
                request_id,
                response,
            }
            .wait(),
            PendingReplaceOutcome::Applied(report)
        );
    }

    #[test]
    fn pending_control_timeout_is_cancelled_before_execution() {
        let state = Arc::new(AtomicU8::new(CONTROL_PENDING));
        let (_response_tx, response_rx) = sync_channel(1);
        let outcome = wait_for_control_result(
            78,
            Arc::clone(&state),
            response_rx,
            Duration::from_millis(1),
            "injected_queue_delay",
        );
        let ControlRequestOutcome::Cancelled(failure) = outcome else {
            panic!("a queued request must be cancelled deterministically");
        };
        assert_eq!(failure.kind, ControlFailureKind::Cancelled);
        assert_eq!(failure.request_id, Some(78));
        assert_eq!(state.load(Ordering::Acquire), CONTROL_CANCELLED);
    }

    #[test]
    fn caps_lock_pending_control_flush_emits_exactly_one_scan_down() {
        let index = RuleIndex::compile(vec![Rule {
            id: "caps-a".to_string(),
            trigger: Trigger::KeyChord {
                first: Key::CapsLock,
                second: Key::A,
            },
            action: Action::KeyChord(vec![Key::C]),
        }])
        .unwrap();
        let matcher = Mutex::new(Matcher::new(Box::new(ManualClock::new(0)), index, 8));
        let caps_down = InputEvent {
            seq: 0,
            time_ms: 0,
            injected: false,
            source: InputSource::Keyboard {
                key: Key::CapsLock,
                scan_code: 0x3a,
                extended: false,
                down: true,
                repeat: false,
            },
        };
        assert_eq!(
            matcher.lock().unwrap().on_event(caps_down).0,
            Decision::Suppress { event_id: 0 }
        );

        // Pause, emergency F12, and clean quit all use this hook-owner flush.
        let report = flush_held_events_on_hook_thread_with(Some(&matcher), |command| {
            let inputs = build_inputs(command);
            assert_eq!(inputs.len(), 1);
            // SAFETY: build_inputs initialized this value as INPUT_KEYBOARD.
            let ki = unsafe { inputs[0].Anonymous.ki };
            assert_eq!((ki.wVk, ki.wScan), (0, 0x3a));
            assert_eq!(ki.dwFlags, KEYEVENTF_SCANCODE);
            OutputReport {
                requested: 1,
                inserted: 1,
                last_error: 0,
                status: OutputStatus::Complete,
            }
        });
        assert_eq!(report.held_events, 1);
        assert!(report.output_complete);
    }

    #[test]
    fn actions_select_vk_or_scan_code_from_match_mode() {
        let logical = make_action_key_input(Key::NumpadEnter, false);
        let physical = make_action_key_input(
            Key::Physical {
                scan_code: 0x1c,
                extended: true,
            },
            false,
        );
        // SAFETY: both values were initialized above as INPUT_KEYBOARD.
        let logical_ki = unsafe { logical.Anonymous.ki };
        // SAFETY: both values were initialized above as INPUT_KEYBOARD.
        let physical_ki = unsafe { physical.Anonymous.ki };
        assert_eq!(logical_ki.wVk, kb::VK_RETURN);
        assert_eq!(logical_ki.wScan, 0);
        assert_eq!(logical_ki.dwFlags, KEYEVENTF_EXTENDEDKEY);
        assert_eq!(physical_ki.wVk, 0);
        assert_eq!(physical_ki.wScan, 0x1c);
        assert_eq!(
            physical_ki.dwFlags,
            KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY
        );
    }

    #[test]
    fn output_fault_injection_distinguishes_zero_partial_and_complete() {
        let command = Command::Replay {
            events: vec![
                key_event(0, Key::LeftCtrl, true),
                key_event(1, Key::A, true),
            ],
        };

        let zero = attempt_output_with(&command, |inputs| {
            assert_eq!(inputs.len(), 2);
            SendCall {
                inserted: 0,
                last_error: 5,
            }
        });
        assert_eq!(zero.status, OutputStatus::ZeroInserted);
        assert_eq!((zero.inserted, zero.requested, zero.last_error), (0, 2, 5));

        let partial = attempt_output_with(&command, |_| SendCall {
            inserted: 1,
            last_error: 0,
        });
        assert_eq!(partial.status, OutputStatus::PartiallyInserted);

        let complete = attempt_output_with(&command, |_| SendCall {
            inserted: 2,
            last_error: 0,
        });
        assert_eq!(complete.status, OutputStatus::Complete);
    }

    #[test]
    fn zero_replay_forwards_only_the_current_event() {
        let first = key_event(0, Key::LeftCtrl, true);
        let current = key_event(1, Key::A, true);
        let command = Command::Replay {
            events: vec![first, current],
        };
        let zero = OutputReport {
            requested: 2,
            inserted: 0,
            last_error: 5,
            status: OutputStatus::ZeroInserted,
        };
        assert_eq!(
            decision_after_output_failure(
                Decision::Suppress { event_id: 1 },
                &command,
                current,
                zero,
            ),
            Decision::PassThrough
        );

        let partial = OutputReport {
            inserted: 1,
            status: OutputStatus::PartiallyInserted,
            ..zero
        };
        assert_eq!(
            decision_after_output_failure(
                Decision::Suppress { event_id: 1 },
                &command,
                current,
                partial,
            ),
            Decision::Suppress { event_id: 1 }
        );
    }

    #[test]
    fn failed_action_never_reanimates_consumed_trigger_input() {
        let current = key_event(7, Key::A, true);
        let action = Command::Emit {
            rule_id: "hold".to_string(),
            action: Action::KeyChord(vec![Key::C]),
        };
        let zero = attempt_output_with(&action, |inputs| {
            assert_eq!(inputs.len(), 2);
            SendCall {
                inserted: 0,
                last_error: 5,
            }
        });
        assert_eq!(
            decision_after_output_failure(
                Decision::Suppress { event_id: 7 },
                &action,
                current,
                zero,
            ),
            Decision::Suppress { event_id: 7 }
        );
    }

    #[test]
    fn pause_flush_uses_the_same_incomplete_output_reporting() {
        let flush = Command::Replay {
            events: vec![key_event(0, Key::F8, true)],
        };
        let report = attempt_output_with(&flush, |inputs| {
            assert_eq!(inputs.len(), 1);
            SendCall {
                inserted: 0,
                last_error: 5,
            }
        });
        assert_eq!(report.status, OutputStatus::ZeroInserted);
        assert_ne!(report.status, OutputStatus::Complete);
    }

    #[test]
    fn legacy_unlock_before_replay_model_proves_overtake() {
        let index = RuleIndex::compile(vec![Rule {
            id: "hold-click".to_string(),
            trigger: Trigger::HoldMouseButton {
                key: Key::LeftCtrl,
                timeout_ms: 250,
                button: MouseButton::Right,
            },
            action: Action::KeyChord(vec![Key::C]),
        }])
        .unwrap();
        let matcher = Arc::new(Mutex::new(Matcher::new(
            Box::new(ManualClock::new(0)),
            index,
            16,
        )));
        matcher
            .lock()
            .unwrap()
            .on_event(key_event(0, Key::LeftCtrl, true));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let matcher_paused = Arc::new(Barrier::new(2));
        let release_replay = Arc::new(Barrier::new(2));

        let pause_matcher = Arc::clone(&matcher);
        let pause_observed = Arc::clone(&observed);
        let pause_ready = Arc::clone(&matcher_paused);
        let pause_release = Arc::clone(&release_replay);
        let pause = std::thread::spawn(move || {
            // This deliberately models the pre-fix production sequence: take
            // pending under the matcher lock, unlock, then perform output.
            let held = pause_matcher.lock().unwrap().set_paused(true);
            pause_ready.wait();
            pause_release.wait();
            pause_observed.lock().unwrap().push(held[0].seq);
        });

        matcher_paused.wait();
        let newer = key_event(1, Key::A, true);
        assert_eq!(
            matcher.lock().unwrap().on_event(newer).0,
            Decision::PassThrough
        );
        observed.lock().unwrap().push(newer.seq);
        release_replay.wait();
        pause.join().unwrap();

        assert_eq!(*observed.lock().unwrap(), vec![1, 0]);
    }

    #[test]
    fn pause_replay_cannot_be_overtaken_on_the_serial_hook_owner() {
        enum Work {
            Pause,
            Input(InputEvent),
            Stop,
        }

        let index = RuleIndex::compile(vec![Rule {
            id: "hold-click".to_string(),
            trigger: Trigger::HoldMouseButton {
                key: Key::LeftCtrl,
                timeout_ms: 250,
                button: MouseButton::Right,
            },
            action: Action::KeyChord(vec![Key::C]),
        }])
        .unwrap();
        let matcher = Arc::new(Mutex::new(Matcher::new(
            Box::new(ManualClock::new(0)),
            index,
            16,
        )));
        assert_eq!(
            matcher
                .lock()
                .unwrap()
                .on_event(key_event(0, Key::LeftCtrl, true))
                .0,
            Decision::Suppress { event_id: 0 }
        );

        let observed = Arc::new(Mutex::new(Vec::new()));
        let dispatch_started = Arc::new(Barrier::new(2));
        let release_dispatch = Arc::new(Barrier::new(2));
        let (work_tx, work_rx) = std::sync::mpsc::channel();
        let (pause_tx, pause_rx) = std::sync::mpsc::channel();
        let (input_tx, input_rx) = std::sync::mpsc::channel();

        let owner_matcher = Arc::clone(&matcher);
        let owner_observed = Arc::clone(&observed);
        let owner_started = Arc::clone(&dispatch_started);
        let owner_release = Arc::clone(&release_dispatch);
        let owner = std::thread::spawn(move || {
            while let Ok(work) = work_rx.recv() {
                match work {
                    Work::Pause => {
                        let report = flush_held_events_on_hook_thread_with(
                            Some(owner_matcher.as_ref()),
                            |command| {
                                let Command::Replay { events } = command else {
                                    panic!("pause must replay held input");
                                };
                                owner_observed.lock().unwrap().push(events[0].seq);
                                owner_started.wait();
                                owner_release.wait();
                                OutputReport {
                                    requested: events.len() as u32,
                                    inserted: events.len() as u32,
                                    last_error: 0,
                                    status: OutputStatus::Complete,
                                }
                            },
                        );
                        pause_tx.send(report).unwrap();
                    }
                    Work::Input(event) => {
                        let decision = owner_matcher.lock().unwrap().on_event(event).0;
                        assert_eq!(decision, Decision::PassThrough);
                        owner_observed.lock().unwrap().push(event.seq);
                        input_tx.send(()).unwrap();
                    }
                    Work::Stop => break,
                }
            }
        });

        work_tx.send(Work::Pause).unwrap();
        dispatch_started.wait();
        work_tx
            .send(Work::Input(key_event(1, Key::A, true)))
            .unwrap();
        assert!(input_rx.recv_timeout(Duration::from_millis(50)).is_err());
        assert_eq!(*observed.lock().unwrap(), vec![0]);

        release_dispatch.wait();
        let report = pause_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(report.output_complete);
        input_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(*observed.lock().unwrap(), vec![0, 1]);

        work_tx.send(Work::Stop).unwrap();
        owner.join().unwrap();
    }

    #[test]
    fn stats_sort_runs_after_releasing_the_recorder_lock() {
        let tracker = Arc::new(Mutex::new(PercentileTracker::new(100_000)));
        {
            let mut guard = tracker.lock().unwrap();
            for value in (0..100_000).rev() {
                guard.record(value);
            }
        }
        let summarize_started = Arc::new(Barrier::new(2));
        let release_summarize = Arc::new(Barrier::new(2));
        let query_tracker = Arc::clone(&tracker);
        let query_started = Arc::clone(&summarize_started);
        let query_release = Arc::clone(&release_summarize);
        let query = std::thread::spawn(move || {
            tracker_summary_with(query_tracker.as_ref(), |snapshot| {
                query_started.wait();
                query_release.wait();
                snapshot.summary()
            })
            .unwrap()
        });

        summarize_started.wait();
        let (recorded_tx, recorded_rx) = std::sync::mpsc::channel();
        let record_tracker = Arc::clone(&tracker);
        let writer = std::thread::spawn(move || {
            record_tracker.lock().unwrap().record(100_001);
            recorded_tx.send(()).unwrap();
        });
        recorded_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("recording must not wait for snapshot sorting");
        release_summarize.wait();

        let summary = query.join().unwrap();
        writer.join().unwrap();
        assert_eq!(summary.total, 100_000);
        assert_eq!(tracker.lock().unwrap().total(), 100_001);
        assert_eq!(summary.p50, Some(49_999));
        assert_eq!(summary.p95, Some(94_999));
        assert_eq!(summary.p99, Some(98_999));
        assert_eq!(summary.max, Some(99_999));
    }

    #[test]
    fn timer_install_failure_is_fatal_before_ready() {
        let error = install_timeout_timer_with(|| (0, 8)).unwrap_err();
        assert!(error.contains("last_error=8"));
        assert!(error.contains("hooks disabled"));
        assert_eq!(install_timeout_timer_with(|| (42, 0)), Ok(42));
    }
}
