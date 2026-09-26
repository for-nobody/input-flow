//! Windows platform layer: low-level keyboard/mouse hooks, the message loop,
//! engine integration, and `SendInput` output.
//!
//! # Safety invariants
//!
//! - All `unsafe` Win32 calls and thread lifecycle are confined to this module.
//! - Hook callbacks dereference `lparam` only when `code >= HC_ACTION`, where the
//!   OS guarantees it points to a valid `KBDLLHOOKSTRUCT` / `MSLLHOOKSTRUCT`.
//! - Callbacks only normalize the event, take a short lock on the matcher, and
//!   non-blocking `try_send` any output; they never allocate unbounded memory,
//!   block on a worker, do I/O, or touch the GUI. `SendInput` runs on a worker.
//! - Hook handles are owned by the hook thread and are uninstalled before that
//!   thread returns. The hook thread creates its message queue (via
//!   `PeekMessageW`) before reporting its thread id, so `PostThreadMessageW`
//!   cannot race the queue creation.
//!
//! M5 behavior: the matcher runs synchronously in the callback; a suppressed
//! event returns non-zero, a passed-through event is forwarded down the chain.
//! Matches emit an action and failures replay the held events, both via
//! `SendInput` on a worker thread with a `dwExtraInfo` tag so our own injected
//! events are recognized and never re-trigger matching. A thread timer
//! (`SetTimer`) drives `Hold` / `Hold+MouseButton` deadlines via `WM_TIMER`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
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
    INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, KF_EXTENDED,
    KF_REPEAT, KF_UP, KillTimer, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, SetTimer, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_QUIT, WM_TIMER,
};

use crate::keymap;

/// Unique `dwExtraInfo` tag written into input synthesized by this program.
pub const SELF_EXTRA_INFO_TAG: usize = 0x494E_5055; // ASCII "INPU"

/// X-button identifiers carried in `MOUSEINPUT.mouseData`.
const XBUTTON1: u32 = 1;
const XBUTTON2: u32 = 2;

/// Timer id for the message-loop timeout timer (drives `Matcher::poll_timeouts`).
const TIMEOUT_TIMER_ID: usize = 1;
/// Polling interval in milliseconds for deadline checks (MVP; M6 may switch to a
/// precise waitable timer).
const TIMEOUT_TIMER_INTERVAL_MS: u32 = 5;

/// The matcher, shared with the hook thread and driven synchronously from the
/// callbacks. Installed once before the hook thread starts.
static MATCHER: OnceLock<Mutex<Matcher>> = OnceLock::new();
/// Sender for output commands (replay / action) consumed by a worker thread.
static OUTPUT_TX: OnceLock<SyncSender<Command>> = OnceLock::new();
/// Sender for human-readable diagnostics consumed by the logger thread.
static LOG_TX: OnceLock<SyncSender<String>> = OnceLock::new();
/// When `true`, no input is intercepted (emergency bypass / output failure).
static BYPASS: AtomicBool = AtomicBool::new(false);
/// Monotonic sequence number assigned to every normalized event.
static SEQ: AtomicU64 = AtomicU64::new(0);
/// Successful `SendInput` output batches.
static OUTPUT_SENT: AtomicU64 = AtomicU64::new(0);
/// Failed `SendInput` output batches (inserted fewer events than requested).
static OUTPUT_FAILED: AtomicU64 = AtomicU64::new(0);
/// Output commands dropped because the bounded channel was full.
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

/// Install the matcher used by the hook callbacks. Call before the hook thread.
pub fn install_matcher(matcher: Matcher) {
    // Only the first call wins; the probe installs it exactly once.
    let _ = MATCHER.set(Mutex::new(matcher));
}

/// Install the output-command sender. Call before the hook thread.
pub fn install_output_sender(tx: SyncSender<Command>) {
    let _ = OUTPUT_TX.set(tx);
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
        dispatch_command(Command::Replay { events: held });
    }
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

/// A latency/delay stats summary: `(total, p50, p95, p99)` in microseconds.
pub type Percentiles = (u64, Option<u64>, Option<u64>, Option<u64>);

/// (total, p50, p95, p99) for callback latency in microseconds, if sampled.
pub fn callback_latency_stats() -> Option<Percentiles> {
    let tracker = callback_latency_tracker().lock().ok()?;
    Some((tracker.total(), tracker.p50(), tracker.p95(), tracker.p99()))
}

/// (total, p50, p95, p99) for hold delay in microseconds, if sampled.
pub fn hold_delay_stats() -> Option<Percentiles> {
    let tracker = hold_delay_tracker().lock().ok()?;
    Some((tracker.total(), tracker.p50(), tracker.p95(), tracker.p99()))
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

/// Number of output commands dropped because the bounded channel was full.
pub fn output_dropped() -> u64 {
    OUTPUT_DROPPED.load(Ordering::Relaxed)
}

/// Execute one output command via `SendInput` and update counters. Must be
/// called from the output worker thread (never from the hook callback).
pub fn execute(command: &Command) -> u32 {
    let inputs = build_inputs(command);
    // SAFETY: `inputs` is fully initialized; `pinputs` points to `len` valid
    // elements; `cbsize` is `size_of::<INPUT>()`.
    let inserted = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            core::mem::size_of::<INPUT>() as i32,
        )
    };
    if inserted == inputs.len() as u32 {
        OUTPUT_SENT.fetch_add(1, Ordering::Relaxed);
    } else {
        OUTPUT_FAILED.fetch_add(1, Ordering::Relaxed);
        send_log(format!(
            "sendinput_failed: inserted {inserted} of {} input(s); entering bypass",
            inputs.len()
        ));
        // Stop intercepting further input when synthesis fails (UIPI, etc.).
        BYPASS.store(true, Ordering::Relaxed);
    }
    inserted
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

fn send_output(command: Command) {
    let Some(tx) = OUTPUT_TX.get() else {
        return;
    };
    match tx.try_send(command) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
            OUTPUT_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Feed one normalized event to the matcher and return the decision. Any
/// resulting replay / action command is forwarded to the output worker. The
/// matcher decision is timed for the performance baseline.
fn process(event: InputEvent) -> Decision {
    let start = Instant::now();
    let Some(matcher) = MATCHER.get() else {
        return Decision::PassThrough;
    };
    let Ok(mut guard) = matcher.lock() else {
        // Poisoned lock: fail safe by passing through.
        return Decision::PassThrough;
    };
    let (decision, resolution) = guard.on_event(event);

    record_callback_latency(start.elapsed().as_micros() as u64);

    // A suppressed down starts (or continues) a hold-delay window.
    if matches!(decision, Decision::Suppress { .. })
        && matches!(resolution, Resolution::Pending)
        && (event.is_key_down() || event.is_button_down())
        && let Ok(mut cell) = hold_start_cell().lock()
        && cell.is_none()
    {
        *cell = Some(start);
    }

    // A queue overflow inside the matcher also stops interception at the
    // platform level so the hook callbacks short-circuit process().
    if guard.is_bypassed() {
        BYPASS.store(true, Ordering::Relaxed);
        send_log("queue_overflow: entering bypass".to_string());
    }

    match resolution {
        Resolution::Matched { rule_id, action } => {
            dispatch_command(Command::Emit { rule_id, action });
        }
        Resolution::Failed { replay } => {
            dispatch_command(Command::Replay { events: replay });
        }
        Resolution::Pending => {}
    }
    decision
}

/// Log and forward one output command to the worker thread. Ends the current
/// hold-delay window so the hold total delay can be sampled.
fn dispatch_command(command: Command) {
    match &command {
        Command::Emit { rule_id, action } => {
            send_log(format!("matched rule `{rule_id}` with {action:?}"));
        }
        Command::Replay { events } => {
            send_log(format!("replay {} held event(s)", events.len()));
        }
    }
    record_hold_delay();
    send_output(command);
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
        dispatch_command(command);
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
                    inputs.push(make_key_input(key, 0, false));
                }
                for &key in keys.iter().rev() {
                    inputs.push(make_key_input(key, 0, true));
                }
                inputs
            }
        },
    }
}

/// Convert one engine event into a Win32 `INPUT`, or `None` for wheel/move.
fn event_to_input(event: &InputEvent) -> Option<INPUT> {
    match event.source {
        InputSource::Keyboard {
            key,
            scan_code,
            down,
            ..
        } => Some(make_key_input(key, scan_code, down)),
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

/// Build one keyboard `INPUT` from an engine key.
// `INPUT` holds a union (`Anonymous`), so `Default` + field assignment is the
// clearest way to set it; the lint is a false positive here.
#[allow(clippy::field_reassign_with_default)]
fn make_key_input(key: Key, scan_code: u16, up: bool) -> INPUT {
    let mut input = INPUT::default();
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki = KEYBDINPUT {
        wVk: keymap::key_to_vk(key),
        wScan: scan_code,
        dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
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

    // SAFETY: GetCurrentThreadId has no preconditions.
    let tid = unsafe { GetCurrentThreadId() };
    if ready.send(Ok(tid)).is_err() {
        // SAFETY: hooks were successfully installed above.
        unsafe { uninstall_hooks(hooks) };
        return;
    }

    // Install a thread timer that posts WM_TIMER to this thread's queue, so the
    // message loop can drive Hold / Hold+MouseButton deadlines. A null hwnd with
    // no callback delivers WM_TIMER through the thread's message queue.
    // SAFETY: null hwnd associates the timer with this thread's message queue
    // (created above); the id is unique to this thread.
    let timer_id = unsafe {
        SetTimer(
            std::ptr::null_mut(),
            TIMEOUT_TIMER_ID,
            TIMEOUT_TIMER_INTERVAL_MS,
            None,
        )
    };
    if timer_id == 0 {
        send_log("failed to install timeout timer; timed rules will not arm".to_string());
    }

    // SAFETY: runs the message loop until GetMessageW returns <= 0.
    unsafe { run_message_loop() };

    if timer_id != 0 {
        // SAFETY: `timer_id` was created by this thread above.
        unsafe { KillTimer(std::ptr::null_mut(), timer_id) };
    }

    // Best-effort flush any held events before removing the hooks.
    flush_held();

    // SAFETY: hooks are still owned by this thread and must be removed before exit.
    unsafe { uninstall_hooks(hooks) };
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

/// `WH_KEYBOARD_LL` callback: normalize, log, then decide suppression via the
/// matcher.
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: `code >= HC_ACTION` guarantees a valid KBDLLHOOKSTRUCT.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let up = (info.flags & KF_UP) != 0;
        let repeat = (info.flags & KF_REPEAT) != 0;
        let vk = info.vkCode as u16;
        let key = keymap::vk_to_key(vk);
        let event = InputEvent {
            seq: next_seq(),
            time_ms: info.time as u64,
            injected: (info.flags & LLKHF_INJECTED) != 0,
            source: InputSource::Keyboard {
                key,
                scan_code: info.scanCode as u16,
                extended: (info.flags & KF_EXTENDED) != 0,
                down: !up,
                repeat,
            },
        };
        if debug_log_enabled() {
            send_log(format!(
                "[seq={:06}] kbd {} {key:?} injected={}",
                event.seq,
                if up { "Up" } else { "Down" },
                event.injected
            ));
        }

        // Our own synthesized events pass through, so replay never recurses.
        if is_own_event(info.dwExtraInfo) {
            // SAFETY: `hhk` is ignored for low-level hooks.
            return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
        }

        // The emergency key toggles suspension on its first (non-repeat) down and
        // is itself never intercepted.
        if !up && !repeat && key == emergency_key() {
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
            return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
        }

        // While bypassed, no input is intercepted.
        if is_bypassed() {
            // SAFETY: `hhk` is ignored for low-level hooks.
            return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
        }

        match process(event) {
            Decision::Suppress { .. } => 1,
            Decision::PassThrough => {
                // SAFETY: `hhk` is ignored for low-level hooks.
                unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
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

            if is_own_event(info.dwExtraInfo) || is_bypassed() {
                // SAFETY: `hhk` is ignored for low-level hooks.
                return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
            }

            match process(event) {
                Decision::Suppress { .. } => 1,
                Decision::PassThrough => {
                    // SAFETY: `hhk` is ignored for low-level hooks.
                    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
                }
            }
        } else {
            // SAFETY: `hhk` is ignored for low-level hooks.
            unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
        }
    } else {
        // SAFETY: `hhk` is ignored for low-level hooks.
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }
}
