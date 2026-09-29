//! Win32 process-shell support for the product agent: current-session single
//! instance ownership and a notification-area icon on its own message thread.
//!
//! # Safety invariants
//!
//! - Every Win32 handle is created, used, and closed by its documented owner.
//! - The tray window and icon are created and destroyed on the tray thread.
//! - The window procedure accesses only process-lifetime synchronization cells;
//!   it never dereferences caller-owned pointers.
//! - Tray callbacks use a bounded `try_send` and never wait on the agent or Hook.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, sync_channel};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentThreadId};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CS_HREDRAW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, IDI_APPLICATION, LoadIconW,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, PostThreadMessageW, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, UnregisterClassW, WM_APP, WM_CONTEXTMENU, WM_LBUTTONDBLCLK,
    WM_NULL, WM_QUIT, WM_RBUTTONUP, WNDCLASSW,
};

const TRAY_ICON_ID: u32 = 1;
const WM_INPUTFLOW_TRAY: u32 = WM_APP + 20;
const WM_INPUTFLOW_REFRESH: u32 = WM_APP + 21;
const MENU_OPEN_SETTINGS: usize = 1001;
const MENU_TOGGLE_PAUSE: usize = 1002;
const MENU_EXIT: usize = 1003;
const MENU_STATUS: usize = 1004;
const EVENT_QUEUE_CAPACITY: usize = 16;

static TRAY_EVENTS: OnceLock<Mutex<Option<SyncSender<TrayEvent>>>> = OnceLock::new();
static TRAY_PAUSED: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleInstance {
    Acquired,
    AlreadyRunning,
}

pub struct SingleInstanceGuard {
    handle: *mut core::ffi::c_void,
}

// The mutex handle is an opaque kernel value and may be held by the main
// thread while other agent threads run; only Drop closes it.
unsafe impl Send for SingleInstanceGuard {}

impl SingleInstanceGuard {
    pub fn acquire(name: &str) -> Result<(SingleInstance, Option<Self>), String> {
        let name = wide(name);
        // SAFETY: the optional security attributes are null and `name` is a
        // valid NUL-terminated buffer for the duration of the call.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            // SAFETY: GetLastError has no preconditions and is read immediately.
            return Err(format!("CreateMutexW failed: {}", unsafe {
                GetLastError()
            }));
        }
        // SAFETY: captured immediately after the successful CreateMutexW call.
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already_running {
            // SAFETY: `handle` is a valid mutex handle returned above.
            unsafe { CloseHandle(handle) };
            Ok((SingleInstance::AlreadyRunning, None))
        } else {
            Ok((SingleInstance::Acquired, Some(Self { handle })))
        }
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: this guard uniquely owns the handle.
            unsafe { CloseHandle(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    OpenSettings,
    TogglePause,
    Exit,
}

pub struct TrayHandle {
    thread_id: u32,
    events: Receiver<TrayEvent>,
    thread: Option<JoinHandle<()>>,
}

impl TrayHandle {
    pub fn start(initially_paused: bool) -> Result<Self, String> {
        TRAY_PAUSED.store(initially_paused, Ordering::Release);
        let (event_tx, event_rx) = sync_channel(EVENT_QUEUE_CAPACITY);
        let cell = TRAY_EVENTS.get_or_init(|| Mutex::new(None));
        *cell
            .lock()
            .map_err(|_| "tray event sender lock is poisoned".to_string())? = Some(event_tx);

        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::spawn(move || tray_thread(ready_tx));
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(Self {
                thread_id,
                events: event_rx,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                clear_tray_sender();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                clear_tray_sender();
                Err("tray thread ended before reporting readiness".to_string())
            }
        }
    }

    pub fn recv(&self) -> Result<TrayEvent, mpsc::RecvError> {
        self.events.recv()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<TrayEvent, mpsc::RecvTimeoutError> {
        self.events.recv_timeout(timeout)
    }

    pub fn set_paused(&self, paused: bool) {
        TRAY_PAUSED.store(paused, Ordering::Release);
        // SAFETY: the tray thread publishes its id only after creating a queue.
        unsafe { PostThreadMessageW(self.thread_id, WM_INPUTFLOW_REFRESH, 0, 0) };
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        if self.thread.is_none() {
            return Ok(());
        }
        // SAFETY: the tray thread publishes its id only after creating a queue.
        if unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) } == 0 {
            // SAFETY: no preconditions; captured immediately after the failure.
            return Err(format!("failed to stop tray thread: {}", unsafe {
                GetLastError()
            }));
        }
        let panicked = self
            .thread
            .take()
            .is_some_and(|thread| thread.join().is_err());
        clear_tray_sender();
        if panicked {
            Err("tray thread panicked".to_string())
        } else {
            Ok(())
        }
    }
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn tray_thread(ready: mpsc::Sender<Result<u32, String>>) {
    let class_name = wide("InputFlow.Agent.Tray.Window");
    let taskbar_message = {
        let name = wide("TaskbarCreated");
        // SAFETY: `name` is a valid NUL-terminated string.
        unsafe { RegisterWindowMessageW(name.as_ptr()) }
    };
    TASKBAR_CREATED.store(taskbar_message, Ordering::Release);
    // SAFETY: null requests the current module handle.
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = WNDCLASSW {
        style: CS_HREDRAW,
        lpfnWndProc: Some(tray_window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };
    // SAFETY: `class` and its name remain valid through registration.
    if unsafe { RegisterClassW(&class) } == 0 {
        let _ = ready.send(Err(format!("RegisterClassW failed: {}", unsafe {
            GetLastError()
        })));
        return;
    }
    // SAFETY: registered class and module handle are valid; this creates an
    // invisible top-level owner window on the current thread.
    let window = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        let error = unsafe { GetLastError() };
        // SAFETY: this thread registered the class above.
        unsafe { UnregisterClassW(class_name.as_ptr(), instance) };
        let _ = ready.send(Err(format!("CreateWindowExW failed: {error}")));
        return;
    }
    if let Err(error) = add_tray_icon(window) {
        // SAFETY: this thread owns both resources.
        unsafe {
            DestroyWindow(window);
            UnregisterClassW(class_name.as_ptr(), instance);
        }
        let _ = ready.send(Err(error));
        return;
    }

    // SAFETY: the message queue exists because CreateWindowExW succeeded.
    let thread_id = unsafe { GetCurrentThreadId() };
    if ready.send(Ok(thread_id)).is_err() {
        delete_tray_icon(window);
        unsafe {
            DestroyWindow(window);
            UnregisterClassW(class_name.as_ptr(), instance);
        }
        return;
    }

    let mut message = MSG::default();
    loop {
        // SAFETY: `message` is valid and null HWND reads this thread's queue.
        let result = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        if message.message == WM_INPUTFLOW_REFRESH {
            let _ = modify_tray_icon(window);
            continue;
        }
        // SAFETY: message was just retrieved from this thread's queue.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    delete_tray_icon(window);
    // SAFETY: this thread owns the window and registered class.
    unsafe {
        DestroyWindow(window);
        UnregisterClassW(class_name.as_ptr(), instance);
    }
}

unsafe extern "system" fn tray_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == TASKBAR_CREATED.load(Ordering::Acquire) {
        let _ = add_tray_icon(window);
        return 0;
    }
    if message == WM_INPUTFLOW_TRAY {
        match lparam as u32 {
            WM_CONTEXTMENU | WM_RBUTTONUP => show_context_menu(window),
            WM_LBUTTONDBLCLK => send_tray_event(TrayEvent::OpenSettings),
            _ => {}
        }
        return 0;
    }
    // SAFETY: unhandled messages are delegated to the system default proc.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn show_context_menu(window: HWND) {
    // SAFETY: creates a menu owned and destroyed in this function.
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    let open = wide("Open Settings");
    let paused = TRAY_PAUSED.load(Ordering::Acquire);
    let status = wide(if paused {
        "Status: Paused"
    } else {
        "Status: Active"
    });
    let toggle = wide(if paused {
        "Resume InputFlow"
    } else {
        "Pause InputFlow"
    });
    let exit = wide("Exit InputFlow");
    // SAFETY: menu is valid and all text buffers live through these calls.
    unsafe {
        AppendMenuW(menu, MF_STRING | MF_GRAYED, MENU_STATUS, status.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, MENU_OPEN_SETTINGS, open.as_ptr());
        AppendMenuW(menu, MF_STRING, MENU_TOGGLE_PAUSE, toggle.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, MENU_EXIT, exit.as_ptr());
        SetForegroundWindow(window);
    }
    let mut point = POINT::default();
    // SAFETY: point is valid; failures leave its zero default.
    unsafe { GetCursorPos(&mut point) };
    // SAFETY: menu/window are valid and TPM_RETURNCMD returns the selected id.
    let selected = unsafe {
        TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            0,
            window,
            std::ptr::null(),
        )
    } as usize;
    // SAFETY: menu is no longer in use.
    unsafe {
        DestroyMenu(menu);
        PostThreadMessageW(GetCurrentThreadId(), WM_NULL, 0, 0);
    }
    match selected {
        MENU_OPEN_SETTINGS => send_tray_event(TrayEvent::OpenSettings),
        MENU_TOGGLE_PAUSE => send_tray_event(TrayEvent::TogglePause),
        MENU_EXIT => send_tray_event(TrayEvent::Exit),
        _ => {}
    }
}

fn add_tray_icon(window: HWND) -> Result<(), String> {
    let data = notification_data(window);
    // SAFETY: `data` is fully initialized and points to the live tray window.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
        Err(format!("Shell_NotifyIconW(NIM_ADD) failed: {}", unsafe {
            GetLastError()
        }))
    } else {
        Ok(())
    }
}

fn modify_tray_icon(window: HWND) -> Result<(), String> {
    let data = notification_data(window);
    // SAFETY: `data` identifies the icon added for this live tray window.
    if unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) } == 0 {
        Err(format!(
            "Shell_NotifyIconW(NIM_MODIFY) failed: {}",
            unsafe { GetLastError() }
        ))
    } else {
        Ok(())
    }
}

fn delete_tray_icon(window: HWND) {
    let data = notification_data(window);
    // SAFETY: deletion is best effort and safe even if Explorer already lost it.
    unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
}

fn notification_data(window: HWND) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: TRAY_ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_INPUTFLOW_TRAY,
        // SAFETY: stock application icon is a shared system resource.
        hIcon: unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) },
        ..Default::default()
    };
    let tooltip = if TRAY_PAUSED.load(Ordering::Acquire) {
        "InputFlow - Paused"
    } else {
        "InputFlow - Active"
    };
    copy_wide(tooltip, &mut data.szTip);
    data
}

fn send_tray_event(event: TrayEvent) {
    if let Some(cell) = TRAY_EVENTS.get()
        && let Ok(sender) = cell.lock()
        && let Some(sender) = sender.as_ref()
    {
        let _ = sender.try_send(event);
    }
}

fn clear_tray_sender() {
    if let Some(cell) = TRAY_EVENTS.get()
        && let Ok(mut sender) = cell.lock()
    {
        *sender = None;
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn copy_wide(value: &str, destination: &mut [u16]) {
    destination.fill(0);
    let capacity = destination.len().saturating_sub(1);
    let encoded = OsStr::new(value).encode_wide();
    for (slot, value) in destination.iter_mut().take(capacity).zip(encoded) {
        *slot = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_copy_is_bounded_and_nul_terminated() {
        let mut target = [0xFFFF; 8];
        copy_wide("0123456789", &mut target);
        assert_eq!(&target[..7], &[48, 49, 50, 51, 52, 53, 54]);
        assert_eq!(target[7], 0);
    }

    #[test]
    fn named_mutex_reports_a_second_instance_and_releases_cleanly() {
        let name = format!(
            "Local\\InputFlow.Agent.Test.{}.{}",
            std::process::id(),
            line!()
        );
        let (first, guard) = SingleInstanceGuard::acquire(&name).unwrap();
        assert_eq!(first, SingleInstance::Acquired);

        let (second, second_guard) = SingleInstanceGuard::acquire(&name).unwrap();
        assert_eq!(second, SingleInstance::AlreadyRunning);
        assert!(second_guard.is_none());

        drop(guard);
        let (third, third_guard) = SingleInstanceGuard::acquire(&name).unwrap();
        assert_eq!(third, SingleInstance::Acquired);
        assert!(third_guard.is_some());
    }
}
