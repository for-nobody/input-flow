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

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use inputflow_engine::{
    Action, Command, Decision, InputEvent, InputSource, Key, Matcher, MouseButton, MouseKind,
    PercentileTracker, Resolution,
};

use windows_sys::Win32::Foundation::{GetLastError, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
    SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, KillTimer,
    LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_QUIT,
    WM_TIMER,
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

/// The matcher, shared with the hook thread and driven synchronously from the
/// callbacks. Installed once before the hook thread starts.
static MATCHER: OnceLock<Mutex<Matcher>> = OnceLock::new();
/// Sender for human-readable diagnostics consumed by the logger thread.
static LOG_TX: OnceLock<SyncSender<String>> = OnceLock::new();
/// When `true`, no input is intercepted (emergency bypass / output failure).
static BYPASS: AtomicBool = AtomicBool::new(false);
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
/// The emergency bypass key (default `F12`), set once before the hook thread.
static EMERGENCY_KEY: OnceLock<Key> = OnceLock::new();
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

/// Install the matcher used by the hook callbacks. Call before the hook thread.
pub fn install_matcher(matcher: Matcher) {
    // Only the first call wins; the probe installs it exactly once.
    let _ = MATCHER.set(Mutex::new(matcher));
}

/// Install the diagnostic-log sender. Call before the hook thread.
pub fn install_log_sender(tx: SyncSender<String>) {
    let _ = LOG_TX.set(tx);
}

/// Install the emergency bypass key. Call before the hook thread starts.
pub fn install_emergency_key(key: Key) {
    let _ = EMERGENCY_KEY.set(key);
}

/// Enable or disable per-event debug logging (off by default).
pub fn set_debug_log(enabled: bool) {
    DEBUG_LOG.store(enabled, Ordering::Relaxed);
}

/// Whether interception is currently stopped (bypass is active).
pub fn is_bypassed() -> bool {
    BYPASS.load(Ordering::Relaxed)
}

/// The configured emergency bypass key (default `F12`).
fn emergency_key() -> Key {
    EMERGENCY_KEY.get().copied().unwrap_or(Key::F12)
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

/// Best-effort flush any held events: pause the matcher (which returns the held
/// events and clears its state) and dispatch them for replay.
fn flush_held_events() {
    let held = match MATCHER.get() {
        Some(matcher) => match matcher.lock() {
            Ok(mut guard) => guard.set_paused(true),
            Err(_) => Vec::new(),
        },
        None => Vec::new(),
    };
    if !held.is_empty() {
        dispatch_command(&Command::Replay { events: held });
    }
}

fn matcher_has_release_tombstones() -> bool {
    MATCHER
        .get()
        .and_then(|matcher| matcher.lock().ok())
        .is_some_and(|guard| guard.has_release_tombstones())
}

/// Suspend interception: flush held events, then stop intercepting new input.
pub fn suspend() {
    flush_held_events();
    BYPASS.store(true, Ordering::Relaxed);
    send_log("suspended: interception stopped (held input flushed)".to_string());
}

/// Resume interception after a suspension, clearing any overflow bypass too.
pub fn resume() {
    BYPASS.store(false, Ordering::Relaxed);
    if let Some(matcher) = MATCHER.get()
        && let Ok(mut guard) = matcher.lock()
    {
        guard.set_paused(false);
        guard.set_bypassed(false);
    }
    send_log("resumed: interception active".to_string());
}

/// Toggle suspension and return the new state (`true` = interception stopped).
pub fn toggle_suspend() -> bool {
    if is_bypassed() {
        resume();
        false
    } else {
        suspend();
        true
    }
}

/// Whether interception is currently suspended (same flag as bypass).
pub fn is_suspended() -> bool {
    is_bypassed()
}

/// Flush any held events; used on clean shutdown before unhooking.
pub fn flush_held() {
    flush_held_events();
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

fn record_callback_latency(micros: u64) {
    if let Ok(mut tracker) = callback_latency_tracker().lock() {
        tracker.record(micros);
    }
}

fn finish_callback(start: Instant, result: LRESULT) -> LRESULT {
    record_callback_latency(start.elapsed().as_micros() as u64);
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

/// Callback wall time, including normalization, matching/output, logging enqueue,
/// and `CallNextHookEx` for forwarded events.
pub fn callback_latency_stats() -> Option<Percentiles> {
    let tracker = callback_latency_tracker().lock().ok()?;
    Some((
        tracker.total(),
        tracker.p50(),
        tracker.p95(),
        tracker.p99(),
        tracker.percentile(100.0),
    ))
}

/// (total, p50, p95, p99, max) for hold delay in microseconds, if sampled.
pub fn hold_delay_stats() -> Option<Percentiles> {
    let tracker = hold_delay_tracker().lock().ok()?;
    Some((
        tracker.total(),
        tracker.p50(),
        tracker.p95(),
        tracker.p99(),
        tracker.percentile(100.0),
    ))
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
        BYPASS.store(true, Ordering::Relaxed);
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
    if let Some(tx) = LOG_TX.get() {
        let _ = tx.try_send(line);
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
        BYPASS.store(true, Ordering::Relaxed);
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
            send_log(format!("matched rule `{rule_id}` with {action:?}"));
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
                    inputs.push(make_key_input(key, 0, false, false));
                }
                for &key in keys.iter().rev() {
                    inputs.push(make_key_input(key, 0, false, true));
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
        } => Some(make_key_input(key, scan_code, extended, down)),
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

/// Build the `KEYBDINPUT.dwFlags` value for a synthesized key event. Replay is
/// VK-based (`wVk` set, `KEYEVENTF_SCANCODE` not set): left/right modifiers are
/// distinguished by their distinct VKs (`VK_LCONTROL` vs `VK_RCONTROL`, …), and
/// the extended-key bit is preserved for replayed physical events so extended
/// keys keep their identity. Pure so it can be unit-tested.
fn keybd_flags(up: bool, extended: bool) -> u32 {
    let mut flags = 0u32;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    flags
}

/// Build one keyboard `INPUT` from an engine key.
// `INPUT` holds a union (`Anonymous`), so `Default` + field assignment is the
// clearest way to set it; the lint is a false positive here.
#[allow(clippy::field_reassign_with_default)]
fn make_key_input(key: Key, scan_code: u16, extended: bool, up: bool) -> INPUT {
    let mut input = INPUT::default();
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki = KEYBDINPUT {
        wVk: keymap::key_to_vk(key),
        wScan: scan_code,
        dwFlags: keybd_flags(up, extended),
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
    if ready.send(Ok(tid)).is_err() {
        // SAFETY: timer and hooks were successfully installed above.
        unsafe {
            KillTimer(std::ptr::null_mut(), timer_id);
            uninstall_hooks(hooks);
        }
        return;
    }

    // SAFETY: runs the message loop until GetMessageW returns <= 0.
    unsafe { run_message_loop() };

    // Best-effort flush any held events before removing the hooks.
    SHUTTING_DOWN.store(true, Ordering::Relaxed);
    flush_held();
    BYPASS.store(true, Ordering::Relaxed);
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
        let key = keymap::vk_to_key(info.vkCode as u16);

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
        let repeat = track_held_key(key, up);

        let event = InputEvent {
            seq: next_seq(),
            time_ms: info.time as u64,
            injected,
            source: InputSource::Keyboard {
                key,
                scan_code: info.scanCode as u16,
                extended,
                down: !up,
                repeat,
            },
        };
        if debug_log_enabled() {
            send_log(format!(
                "[seq={:06}] kbd {} {key:?} injected={injected} repeat={repeat}",
                event.seq,
                if up { "Up" } else { "Down" },
            ));
        }

        // The emergency key toggles suspension on its first (non-repeat) down and
        // is itself never intercepted.
        if !up && !repeat && key == emergency_key() && !SHUTTING_DOWN.load(Ordering::Relaxed) {
            let suspended = toggle_suspend();
            send_log(format!(
                "suspended {}",
                if suspended {
                    "ON (interception stopped)"
                } else {
                    "OFF (interception active)"
                }
            ));
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
        assert_eq!(keybd_flags(false, false), 0);
        assert_eq!(keybd_flags(true, false), KEYEVENTF_KEYUP);
        assert_eq!(keybd_flags(false, true), KEYEVENTF_EXTENDEDKEY);
        assert_eq!(
            keybd_flags(true, true),
            KEYEVENTF_KEYUP | KEYEVENTF_EXTENDEDKEY
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
    fn timer_install_failure_is_fatal_before_ready() {
        let error = install_timeout_timer_with(|| (0, 8)).unwrap_err();
        assert!(error.contains("last_error=8"));
        assert!(error.contains("hooks disabled"));
        assert_eq!(install_timeout_timer_with(|| (42, 0)), Ok(42));
    }
}
