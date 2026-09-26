//! Windows platform layer: low-level keyboard/mouse hooks, the message loop, and
//! `SendInput` replay.
//!
//! # Safety invariants
//!
//! - All `unsafe` Win32 calls and thread lifecycle are confined to this module.
//! - Hook callbacks dereference `lparam` only when `code >= HC_ACTION`, where the
//!   OS guarantees it points to a valid `KBDLLHOOKSTRUCT` / `MSLLHOOKSTRUCT`.
//! - Callbacks only read `Copy` fields, perform lightweight atomic/state checks,
//!   and non-blocking `try_send` requests; they never allocate, block, wait on a
//!   worker, do I/O, or touch the GUI. Replay runs on a worker thread.
//! - Hook handles are owned by the hook thread and are uninstalled before that
//!   thread returns. The hook thread creates its message queue (via `PeekMessageW`)
//!   before reporting its thread id, so `PostThreadMessageW(WM_QUIT)` cannot race
//!   the queue creation.
//!
//! M2 behavior: physical F8 down/up is suppressed (the callback returns non-zero);
//! a delayed, tag-marked F8 down+up pair is synthesized with `SendInput` on a
//! worker thread. The `dwExtraInfo` tag lets the callback recognize its own
//! injected events and pass them through, preventing recursion. F12 toggles a
//! bypass flag that stops all interception; `SendInput` failures set bypass.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};

use windows_sys::Win32::Foundation::{GetLastError, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, KF_EXTENDED,
    KF_REPEAT, KF_UP, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::event::{
    EMERGENCY_KEY_VK, HELD_KEY_VK, HookMessage, InputEvent, MouseKind, SELF_EXTRA_INFO_TAG,
    is_own_event,
};

/// X-button identifiers carried in the high word of `MSLLHOOKSTRUCT.mouseData`.
const XBUTTON1: u16 = 1;
const XBUTTON2: u16 = 2;

/// A request to synthesize one held-key press after a short delay.
#[derive(Debug, Clone, Copy)]
pub struct ReplayRequest {
    /// Sequence number of the suppressed physical down event this replay belongs to.
    pub seq: u64,
}

/// Sender shared with the hook callbacks. Set once by `install_event_sender`
/// before the hook thread is spawned. Carries both normalized events and
/// lightweight control notes (bypass toggles) for the logger thread.
static EVENT_TX: OnceLock<SyncSender<HookMessage>> = OnceLock::new();
/// Sender for delayed replay requests, set once by `install_replay_sender`.
static REPLAY_TX: OnceLock<SyncSender<ReplayRequest>> = OnceLock::new();
/// Monotonic sequence number assigned to every normalized event, preserving
/// cross-device ordering as observed by the hook thread.
static SEQ: AtomicU64 = AtomicU64::new(0);
/// Number of messages dropped because the bounded queue was full.
static DROPPED: AtomicU64 = AtomicU64::new(0);
/// When `true`, no input is intercepted; F8 passes through unchanged.
static BYPASS: AtomicBool = AtomicBool::new(false);
/// Successful replays (SendInput inserted the full down+up pair).
static REPLAY_SENT: AtomicU64 = AtomicU64::new(0);
/// Failed replays (SendInput inserted fewer events than requested).
static REPLAY_FAILED: AtomicU64 = AtomicU64::new(0);
/// Replay requests dropped because the bounded replay queue was full.
static REPLAY_DROPPED: AtomicU64 = AtomicU64::new(0);

/// Install the shared event sender. Must be called before spawning the hook thread.
pub fn install_event_sender(tx: SyncSender<HookMessage>) {
    // Only the first call wins; main installs it exactly once.
    let _ = EVENT_TX.set(tx);
}

/// Install the shared replay sender. Must be called before spawning the hook thread.
pub fn install_replay_sender(tx: SyncSender<ReplayRequest>) {
    let _ = REPLAY_TX.set(tx);
}

/// Number of events that were normalized (assigned a sequence number).
pub fn seq_count() -> u64 {
    SEQ.load(Ordering::Relaxed)
}

/// Number of events dropped because the bounded queue was full.
pub fn dropped_count() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// Whether interception is currently stopped (bypass is active).
pub fn is_bypassed() -> bool {
    BYPASS.load(Ordering::Relaxed)
}

/// Number of successful replays.
pub fn replay_sent() -> u64 {
    REPLAY_SENT.load(Ordering::Relaxed)
}

/// Number of failed replays (SendInput inserted fewer events than requested).
pub fn replay_failed() -> u64 {
    REPLAY_FAILED.load(Ordering::Relaxed)
}

/// Number of replay requests dropped because the bounded queue was full.
pub fn replay_dropped() -> u64 {
    REPLAY_DROPPED.load(Ordering::Relaxed)
}

/// Replay the held key via `SendInput` and update counters/bypass based on the
/// result. Returns the number of events `SendInput` reported as inserted.
pub fn replay_held_key() -> u32 {
    let inserted = send_f8_press();
    if inserted == 2 {
        REPLAY_SENT.fetch_add(1, Ordering::Relaxed);
    } else {
        REPLAY_FAILED.fetch_add(1, Ordering::Relaxed);
        // Stop intercepting further input when synthesis fails.
        BYPASS.store(true, Ordering::Relaxed);
    }
    inserted
}

/// Toggle the bypass flag and return the new value (`true` = interception stopped).
fn toggle_bypass() -> bool {
    let prev = BYPASS.fetch_xor(true, Ordering::Relaxed);
    !prev
}

/// Synthesize one held-key press (down followed immediately by up) via `SendInput`,
/// tagged with `SELF_EXTRA_INFO_TAG` so the hook can recognize it as our own.
/// Returns the number of events `SendInput` reported as inserted.
fn send_f8_press() -> u32 {
    // SAFETY: the INPUT array is fully initialized; `pinputs` points to `cinputs`
    // valid elements; `cbsize` is `size_of::<INPUT>()`.
    unsafe {
        let inputs = [make_f8_input(false), make_f8_input(true)];
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            core::mem::size_of::<INPUT>() as i32,
        )
    }
}

/// Build one keyboard `INPUT` for F8, either a down or an up event.
fn make_f8_input(up: bool) -> INPUT {
    let mut input = INPUT::default();
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki = KEYBDINPUT {
        wVk: HELD_KEY_VK,
        wScan: 0,
        dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
        time: 0,
        dwExtraInfo: SELF_EXTRA_INFO_TAG,
    };
    input
}

/// Post `WM_QUIT` to the hook thread. Returns `false` if the call failed.
pub fn post_quit(hook_thread_id: u32) -> bool {
    // SAFETY: `hook_thread_id` belongs to the hook thread, whose message queue was
    // created (via PeekMessageW) before it reported the id. WM_QUIT is a valid message.
    let ret = unsafe { PostThreadMessageW(hook_thread_id, WM_QUIT, 0, 0) };
    ret != 0
}

/// Entry point for the dedicated hook thread. Reports the thread id (or an error
/// string) through `ready`, installs both hooks, runs the message loop, and
/// uninstalls the hooks before returning.
pub fn run_hook_thread(ready: std::sync::mpsc::Sender<Result<u32, String>>) {
    // SAFETY: install_hooks is only called on this thread and documents its own
    // preconditions; hook handles are returned to be cleaned up below.
    let hooks = match unsafe { install_hooks() } {
        Ok(hooks) => hooks,
        Err(err) => {
            let _ = ready.send(Err(err));
            return;
        }
    };

    // Create this thread's message queue before signaling readiness. Low-level
    // hooks are delivered by posting to the installer thread's queue, so the
    // queue must exist before the main thread can post WM_QUIT.
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid pointer to a zeroed MSG; null hwnd queries this
    // thread's queue. PM_NOREMOVE creates the queue without consuming a message.
    unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };

    // SAFETY: GetCurrentThreadId has no preconditions.
    let tid = unsafe { GetCurrentThreadId() };
    if ready.send(Ok(tid)).is_err() {
        // The main thread is already gone; clean up and exit.
        // SAFETY: hooks were successfully installed above.
        unsafe { uninstall_hooks(hooks) };
        return;
    }

    // SAFETY: runs the message loop until GetMessageW returns <= 0 (WM_QUIT or error).
    unsafe { run_message_loop() };

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
/// Must be called from the thread that will run the message loop, because
/// low-level hooks are delivered on the installing thread.
unsafe fn install_hooks() -> Result<HookHandles, String> {
    // SAFETY: a null module name returns the handle of the current executable,
    // which is where the hook procedures live.
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
        // this thread. The return value is 0 on WM_QUIT, negative on error, and
        // positive when a message was retrieved.
        let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if ret <= 0 {
            break;
        }
        // Low-level hooks are invoked while messages are retrieved; translating
        // and dispatching is a no-op for this windowless thread but kept for
        // conventional correctness.
        // SAFETY: `msg` was just retrieved and is a valid, initialized message.
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
/// Assign the next monotonic sequence number.
fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Non-blocking enqueue of a normalized input event for the logger.
fn enqueue(event: InputEvent) {
    send_message(HookMessage::Input(event));
}

/// Non-blocking send of a bypass-state note to the logger.
fn send_bypass_note(bypass: bool) {
    send_message(HookMessage::Bypass(bypass));
}

/// Non-blocking send on the shared message channel, counting drops when full.
fn send_message(message: HookMessage) {
    let Some(tx) = EVENT_TX.get() else {
        return;
    };
    match tx.try_send(message) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Disconnected(_)) => {}
    }
}

/// Non-blocking enqueue of a delayed replay request for the replay worker.
fn send_replay_request(req: ReplayRequest) {
    let Some(tx) = REPLAY_TX.get() else {
        return;
    };
    match tx.try_send(req) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            REPLAY_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Disconnected(_)) => {}
    }
}

/// `WH_KEYBOARD_LL` callback: normalize, log, then decide suppression/replay.
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: `code >= HC_ACTION` guarantees `lparam` points to a valid
        // KBDLLHOOKSTRUCT for the duration of the call.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let seq = next_seq();
        let up = (info.flags & KF_UP) != 0;
        let repeat = (info.flags & KF_REPEAT) != 0;
        let vk = info.vkCode as u16;
        let extra_info = info.dwExtraInfo;

        enqueue(InputEvent::Keyboard {
            seq,
            time_ms: info.time,
            up,
            vk,
            scan: info.scanCode as u16,
            extended: (info.flags & KF_EXTENDED) != 0,
            repeat,
            injected: (info.flags & LLKHF_INJECTED) != 0,
            extra_info,
        });

        // Our own synthesized events pass through (recognized by the
        // `dwExtraInfo` tag) so replay never re-triggers suppression.
        if is_own_event(extra_info) {
            // SAFETY: `hhk` is ignored for low-level hooks; pass the event down.
            return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
        }

        // The emergency key toggles bypass on its first (non-repeat) down and is
        // itself never intercepted.
        if !up && !repeat && vk == EMERGENCY_KEY_VK {
            let bypass = toggle_bypass();
            send_bypass_note(bypass);
            // SAFETY: `hhk` is ignored for low-level hooks; pass the event down.
            return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
        }

        // While bypassed, no input is intercepted.
        if is_bypassed() {
            // SAFETY: `hhk` is ignored for low-level hooks; pass the event down.
            return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
        }

        // Suppress the held key: the down schedules a delayed replay, the up is
        // swallowed so the target never sees an orphan release.
        if vk == HELD_KEY_VK {
            if !up && !repeat {
                send_replay_request(ReplayRequest { seq });
            }
            return 1; // non-zero suppresses the event
        }
    }
    // SAFETY: `hhk` is ignored for low-level hooks; pass the event down the chain.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// `WH_MOUSE_LL` callback: normalize and enqueue, then pass the event through.
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: `code >= HC_ACTION` guarantees `lparam` points to a valid
        // MSLLHOOKSTRUCT for the duration of the call.
        let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let injected = (info.flags & LLMHF_INJECTED) != 0;
        let kind = match wparam as u32 {
            WM_MOUSEMOVE => None, // skipped to avoid flooding the probe
            WM_LBUTTONDOWN => Some(MouseKind::LeftDown),
            WM_LBUTTONUP => Some(MouseKind::LeftUp),
            WM_RBUTTONDOWN => Some(MouseKind::RightDown),
            WM_RBUTTONUP => Some(MouseKind::RightUp),
            WM_MBUTTONDOWN => Some(MouseKind::MiddleDown),
            WM_MBUTTONUP => Some(MouseKind::MiddleUp),
            WM_XBUTTONDOWN => match high_word(info.mouseData) {
                XBUTTON1 => Some(MouseKind::XButton1Down),
                XBUTTON2 => Some(MouseKind::XButton2Down),
                _ => None,
            },
            WM_XBUTTONUP => match high_word(info.mouseData) {
                XBUTTON1 => Some(MouseKind::XButton1Up),
                XBUTTON2 => Some(MouseKind::XButton2Up),
                _ => None,
            },
            WM_MOUSEWHEEL => Some(MouseKind::Wheel {
                delta: wheel_delta(info.mouseData),
            }),
            WM_MOUSEHWHEEL => Some(MouseKind::HorizontalWheel {
                delta: wheel_delta(info.mouseData),
            }),
            _ => None,
        };

        if let Some(kind) = kind {
            let event = InputEvent::Mouse {
                seq: next_seq(),
                time_ms: info.time,
                kind,
                x: info.pt.x,
                y: info.pt.y,
                injected,
                extra_info: info.dwExtraInfo,
            };
            enqueue(event);
        }
    }
    // SAFETY: `hhk` is ignored for low-level hooks; pass the event down the chain.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// High 16 bits of a 32-bit value.
fn high_word(value: u32) -> u16 {
    (value >> 16) as u16
}

/// Signed wheel delta stored in the high word of `mouseData` (120 == one notch).
fn wheel_delta(value: u32) -> i32 {
    (high_word(value) as i16) as i32
}
