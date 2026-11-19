//! Background Raw Input and Touch Click. The hook thread commits queued clicks.
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use inputflow_config::TouchpadConfig;
use inputflow_engine::MouseButton;
use inputflow_touchpad::contact::{Frame, Tracker, Transition};
use inputflow_touchpad::gesture::{Decision, Event, Parameters, Recognizer, Scale};
use inputflow_touchpad::hid::{Decoded, Decoder, Descriptor};
use inputflow_touchpad::raw::{device_info, raw_devices, read_packet};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::Input::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::core::w;

use super::windows;

const PAGE: u16 = 0x0d;
const USAGE: u16 = 0x05;
const TIMER: usize = 1;
const TICK_MS: u32 = 12;
const MAX_PENDING: usize = 16;
const CONFIG_CHANGED: u32 = WM_APP + 31;
pub(super) const CLICK_READY: u32 = WM_APP + 32;

static SETTINGS: Mutex<Option<TouchpadConfig>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(1);
static READY: AtomicBool = AtomicBool::new(false);
static CONFIG_PENDING: AtomicBool = AtomicBool::new(false);
static FAULT: AtomicBool = AtomicBool::new(false);
static RECOVERY_WAITING: AtomicBool = AtomicBool::new(false);
static WINDOW_THREAD: AtomicU64 = AtomicU64::new(0);
static CLICKS: Mutex<VecDeque<Click>> = Mutex::new(VecDeque::new());
static STATUS: Mutex<TouchpadStatus> = Mutex::new(TouchpadStatus::new());

thread_local! {
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchpadStatus {
    pub experimental_enabled: bool,
    pub click_enabled: bool,
    pub device_available: bool,
    pub output_allowed: bool,
    pub reason: &'static str,
    pub reports: u64,
    pub sink_reports: u64,
    pub complete_contacts: u64,
    pub short_intents: u64,
    pub long_intents: u64,
    pub requested_inputs: u64,
    pub inserted_inputs: u64,
    pub last_cancel: Option<&'static str>,
    pub normalized_distance: bool,
}

impl TouchpadStatus {
    const fn new() -> Self {
        Self {
            experimental_enabled: false,
            click_enabled: false,
            device_available: false,
            output_allowed: false,
            reason: "disabled",
            reports: 0,
            sink_reports: 0,
            complete_contacts: 0,
            short_intents: 0,
            long_intents: 0,
            requested_inputs: 0,
            inserted_inputs: 0,
            last_cancel: None,
            normalized_distance: false,
        }
    }
}

impl Default for TouchpadStatus {
    fn default() -> Self {
        Self::new()
    }
}

fn update_status(update: impl FnOnce(&mut TouchpadStatus)) {
    if let Ok(mut status) = STATUS.lock() {
        update(&mut status);
    }
}

pub fn status() -> TouchpadStatus {
    let mut status = STATUS
        .lock()
        .map_or(TouchpadStatus::new(), |status| *status);
    status.output_allowed &= READY.load(Ordering::Acquire)
        && !CONFIG_PENDING.load(Ordering::Acquire)
        && !windows::is_bypassed()
        && !windows::is_capture_active();
    if FAULT.load(Ordering::Acquire) {
        status.reason = "output_fault";
    } else if CONFIG_PENDING.load(Ordering::Acquire) {
        status.reason = "config_pending";
    } else if status.experimental_enabled && status.click_enabled {
        if windows::is_capture_active() {
            status.reason = "recording";
        } else if windows::is_bypassed() {
            status.reason = "paused";
        }
    }
    status
}

fn wake_worker() {
    let thread = WINDOW_THREAD.load(Ordering::Acquire) as u32;
    if thread != 0 {
        // SAFETY: The id is published after the thread has created its window and queue.
        unsafe { PostThreadMessageW(thread, CONFIG_CHANGED, 0, 0) };
    }
}

/// Also called from the input hook. No locks or device operations on this path.
pub(super) fn invalidate() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub(super) fn acknowledge_recovery() {
    if RECOVERY_WAITING.swap(false, Ordering::AcqRel) {
        invalidate();
        wake_worker();
    }
}

pub fn set_config_pending(pending: bool) {
    if CONFIG_PENDING.swap(pending, Ordering::AcqRel) != pending {
        invalidate();
        wake_worker();
    }
}

pub fn configure(config: TouchpadConfig) {
    READY.store(false, Ordering::Release);
    if let Ok(mut settings) = SETTINGS.lock() {
        *settings = Some(config);
        invalidate();
    }
    update_status(|status| {
        status.experimental_enabled = config.experimental_enabled;
        status.click_enabled = config.click_enabled;
        status.output_allowed = false;
    });
    wake_worker();
}

pub struct TouchpadThread {
    id: u32,
    handle: Option<JoinHandle<()>>,
}

impl TouchpadThread {
    pub fn start(
        config: TouchpadConfig,
        observe: bool,
        recovery_waiting: bool,
    ) -> Result<Self, String> {
        READY.store(false, Ordering::Release);
        FAULT.store(false, Ordering::Release);
        RECOVERY_WAITING.store(recovery_waiting, Ordering::Release);
        if let Ok(mut clicks) = CLICKS.lock() {
            clicks.clear();
        }
        CONFIG_PENDING.store(false, Ordering::Release);
        update_status(|status| *status = TouchpadStatus::new());
        configure(config);
        let (tx, rx) = mpsc::sync_channel(1);
        let handle = std::thread::spawn(move || run_window(observe, tx));
        match rx.recv() {
            Ok(Ok(id)) => Ok(Self {
                id,
                handle: Some(handle),
            }),
            ready => {
                let _ = handle.join();
                update_status(|status| status.reason = "registration_failed");
                Err(match ready {
                    Ok(Err(error)) => error,
                    _ => "touchpad message thread exited during startup".to_string(),
                })
            }
        }
    }
}

impl Drop for TouchpadThread {
    fn drop(&mut self) {
        READY.store(false, Ordering::Release);
        invalidate();
        // SAFETY: This thread id was reported after its queue was created.
        if unsafe { PostThreadMessageW(self.id, WM_QUIT, 0, 0) } != 0
            && let Some(handle) = self.handle.take()
        {
            let _ = handle.join();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Target {
    foreground: usize,
    hit: usize,
}

impl Target {
    fn current() -> Option<Self> {
        let mut cursor = POINT::default();
        // SAFETY: The system APIs return handles or write to the initialized point.
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.is_null() || GetCursorPos(&mut cursor) == 0 {
                return None;
            }
            let hit = WindowFromPoint(cursor);
            (!hit.is_null()).then_some(Self {
                foreground: foreground as usize,
                hit: hit as usize,
            })
        }
    }
}

struct Click {
    generation: u64,
    valid: Arc<AtomicBool>,
    target: Target,
    button: MouseButton,
}

impl Click {
    fn can_send(&self) -> bool {
        READY.load(Ordering::Acquire)
            && !FAULT.load(Ordering::Acquire)
            && !RECOVERY_WAITING.load(Ordering::Acquire)
            && !CONFIG_PENDING.load(Ordering::Acquire)
            && !windows::is_bypassed()
            && !windows::is_capture_active()
            && self.generation == GENERATION.load(Ordering::Acquire)
            && self.valid.load(Ordering::Acquire)
            && Target::current() == Some(self.target)
            && !physical_mouse_pressed()
    }
}

pub(super) fn process_clicks() {
    let click = CLICKS.lock().ok().and_then(|mut queue| queue.pop_front());
    let Some(click) = click else { return };
    if !native_taps_disabled() || !click.can_send() {
        update_status(|status| status.last_cancel = Some("output_guard"));
        return;
    }
    if let Some(report) = windows::send_touchpad_click(click.button, || click.can_send()) {
        update_status(|status| {
            status.requested_inputs += 2;
            status.inserted_inputs += u64::from(report.inserted);
        });
        if report.inserted != 2 {
            FAULT.store(true, Ordering::Release);
            READY.store(false, Ordering::Release);
            invalidate();
            wake_worker();
            windows::send_log(format!(
                "touchpad_output_fault requested=2 inserted={} release_repair={:?}",
                report.inserted, report.repair
            ));
        }
    }
}

fn queue_click(click: Click) {
    if let Ok(mut queue) = CLICKS.lock() {
        if queue.len() >= MAX_PENDING {
            update_status(|status| status.last_cancel = Some("output_queue_full"));
            return;
        }
        queue.push_back(click);
        if !windows::wake_touchpad_output() {
            queue.pop_back();
            update_status(|status| status.last_cancel = Some("hook_unavailable"));
        }
    }
}

fn physical_mouse_pressed() -> bool {
    // SAFETY: The arguments are Win32 virtual keys for mouse buttons.
    [1, 2, 4, 5, 6]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(key) < 0 })
}

fn native_taps_disabled() -> bool {
    let mut value = 1u32;
    let mut bytes = size_of::<u32>() as u32;
    // SAFETY: The subkey and value names are NUL-terminated; value is writable DWORD storage.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\PrecisionTouchPad"),
            w!("TapsEnabled"),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut value as *mut u32).cast(),
            &mut bytes,
        )
    };
    result == ERROR_SUCCESS && value == 0
}

struct Gesture {
    generation: u64,
    valid: Arc<AtomicBool>,
    target: Option<Target>,
}

struct Device {
    decoder: Decoder,
    scale: Scale,
    tracker: Tracker,
    recognizer: Recognizer,
    gesture: Option<Gesture>,
    occupied: bool,
}

impl Device {
    fn new(decoder: Decoder, scale: Scale) -> Self {
        let mut tracker = Tracker::default();
        tracker.interrupt("awaiting_clear");
        Self {
            decoder,
            scale,
            tracker,
            recognizer: Recognizer::default(),
            gesture: None,
            occupied: false,
        }
    }

    fn cancel(&mut self, reason: &'static str, now: Instant, observe: bool) {
        if let Some(gesture) = self.gesture.take() {
            gesture.valid.store(false, Ordering::Release);
        }
        self.tracker.interrupt(reason);
        if let Some((Some(event), _)) = self.recognizer.finish(None, now, reason) {
            self.record(event, false, observe);
        }
    }

    fn guard(&mut self, generation: u64, now: Instant, observe: bool) {
        if let Some(gesture) = &self.gesture {
            let reason = if gesture.generation != generation {
                Some("state_changed")
            } else if gesture.target != Target::current() {
                Some("target_changed")
            } else {
                None
            };
            if let Some(reason) = reason {
                self.cancel(reason, now, observe);
            }
        }
    }

    fn frame(&mut self, frame: Decoded, context: &FrameContext) {
        let FrameContext {
            now,
            generation,
            parameters,
            blocked,
            output,
            observe,
        } = *context;
        self.guard(generation, now, observe);
        if blocked || matches!(frame.frame, Frame::Multiple) || frame.physical_button == Some(true)
        {
            let reason = if blocked || matches!(frame.frame, Frame::Multiple) {
                "multiple_contacts"
            } else {
                "physical_button"
            };
            self.cancel(reason, now, observe);
            return;
        }
        if self.decoder.has_physical_button() && frame.physical_button.is_none() {
            self.cancel("unknown_button_state", now, observe);
            return;
        }
        let transition = self.tracker.observe(frame.frame, now);
        if let Some(Transition::Down { id, start }) = transition {
            self.recognizer
                .begin(id, start, now, self.scale, parameters);
            self.gesture = Some(Gesture {
                generation,
                valid: Arc::new(AtomicBool::new(true)),
                target: Target::current(),
            });
            return;
        }
        if let Some(Transition::Incomplete { reason, .. }) = transition {
            self.cancel(reason, now, observe);
            return;
        }
        if let Frame::Single(contact) = frame.frame
            && let Some(event) = self.recognizer.position(contact.point, now)
        {
            if let Some(gesture) = self.gesture.as_ref() {
                gesture.valid.store(false, Ordering::Release);
            }
            self.record(event, false, observe);
        }
        if let Some(Transition::Up { end, .. }) = transition {
            update_status(|status| status.complete_contacts += 1);
            if let Some((Some(event), _)) = self.recognizer.finish(Some(end), now, "missing_up") {
                self.record(
                    event,
                    output && frame.physical_button == Some(false),
                    observe,
                );
            }
            self.gesture = None;
        }
    }

    fn tick(&mut self, generation: u64, output: bool, observe: bool, now: Instant) {
        self.guard(generation, now, observe);
        if let Some((token, _)) = self.recognizer.deadline()
            && let Some(event) = self.recognizer.timer(token, now)
        {
            self.record(event, output, observe);
        }
    }

    fn record(&self, event: Event, output: bool, observe: bool) {
        update_status(|status| match event.decision {
            Decision::Short => status.short_intents += 1,
            Decision::Long => status.long_intents += 1,
            Decision::Suppress(reason) | Decision::AfterLong(reason) => {
                status.last_cancel = Some(reason)
            }
        });
        if observe {
            windows::send_log(format!(
                "touchpad_observe elapsed_ms={} decision={} reason={}",
                event.elapsed.as_millis(),
                event.decision.label(),
                event.decision.reason()
            ));
        }
        let button = match event.decision {
            Decision::Short => MouseButton::Left,
            Decision::Long => MouseButton::Right,
            _ => return,
        };
        if output
            && self.decoder.has_physical_button()
            && self.scale.millimeters.is_some()
            && let Some(gesture) = &self.gesture
            && let Some(target) = gesture.target
        {
            queue_click(Click {
                generation: gesture.generation,
                valid: Arc::clone(&gesture.valid),
                target,
                button,
            });
        }
    }
}

struct FrameContext {
    now: Instant,
    generation: u64,
    parameters: Parameters,
    blocked: bool,
    output: bool,
    observe: bool,
}

struct Worker {
    window: HWND,
    devices: HashMap<usize, Device>,
    buffer: Vec<usize>,
    registered: bool,
    ticking: bool,
    observe: bool,
    session_notifications: bool,
    sleeping: bool,
    locked: bool,
    generation: u64,
    output: bool,
    native_off: bool,
    checked_taps: Instant,
    parameters: Parameters,
}

impl Worker {
    fn refresh(&mut self, now: Instant) {
        let (config, configured_generation) = SETTINGS.lock().map_or_else(
            |_| (TouchpadConfig::default(), 0),
            |settings| {
                (
                    settings.unwrap_or_default(),
                    GENERATION.load(Ordering::Acquire),
                )
            },
        );
        let paused = windows::is_bypassed() || windows::is_capture_active();
        let active = self.observe || (config.experimental_enabled && config.click_enabled);
        let should_tick = active && !self.sleeping && !self.locked;
        if should_tick && !self.ticking {
            // SAFETY: This worker owns the live window and its timer.
            self.ticking = unsafe { SetTimer(self.window, TIMER, TICK_MS, None) } != 0;
        } else if !should_tick && self.ticking {
            // SAFETY: This is the timer installed on the same window and thread.
            unsafe { KillTimer(self.window, TIMER) };
            self.ticking = false;
        }
        let subscribe = should_tick && self.ticking;
        let mut registration_failed = should_tick && !self.ticking;
        if subscribe != self.registered {
            match register(self.window, subscribe) {
                Ok(()) => {
                    self.registered = subscribe;
                    if subscribe {
                        self.enumerate();
                    } else {
                        self.devices.clear();
                    }
                    windows::send_log(format!(
                        "touchpad_subscription active={subscribe} devices={} mode=input_sink",
                        self.devices.len()
                    ));
                    invalidate();
                }
                Err(_) => registration_failed = true,
            }
        }

        if active && now.duration_since(self.checked_taps) >= Duration::from_millis(250) {
            let native_off = native_taps_disabled();
            if native_off != self.native_off {
                self.native_off = native_off;
                invalidate();
            }
            self.checked_taps = now;
        }
        let capable = self.devices.values().any(|device| {
            device.decoder.has_physical_button() && device.scale.millimeters.is_some()
        });
        let reason = if registration_failed {
            "registration_failed"
        } else if self.observe {
            "observe_only"
        } else if !config.experimental_enabled {
            "disabled"
        } else if !config.click_enabled {
            "rule_disabled"
        } else if FAULT.load(Ordering::Acquire) {
            "output_fault"
        } else if RECOVERY_WAITING.load(Ordering::Acquire) {
            "recovery_waiting"
        } else if CONFIG_PENDING.load(Ordering::Acquire) {
            "config_pending"
        } else if self.locked || self.sleeping {
            "session_inactive"
        } else if paused {
            "paused"
        } else if self.devices.is_empty() {
            "device_unavailable"
        } else if !capable {
            "device_read_only"
        } else if !self.session_notifications {
            "session_notifications_unavailable"
        } else if !self.native_off {
            "native_tap_enabled"
        } else {
            "ready"
        };
        self.output =
            reason == "ready" && configured_generation == GENERATION.load(Ordering::Acquire);
        READY.store(self.output, Ordering::Release);
        self.parameters = Parameters {
            short: Duration::from_millis(config.short_ms),
            hold: Duration::from_millis(config.hold_ms),
            movement_mm: config.movement_mm,
            ..Parameters::default()
        };
        let generation = GENERATION.load(Ordering::Acquire);
        if self.generation != generation || paused {
            for device in self.devices.values_mut() {
                device.cancel(
                    if paused { "paused" } else { "state_changed" },
                    now,
                    self.observe,
                );
            }
            self.generation = generation;
        }
        update_status(|status| {
            status.device_available = !self.devices.is_empty();
            status.output_allowed = self.output;
            status.reason = reason;
            status.normalized_distance = self
                .devices
                .values()
                .any(|device| device.scale.millimeters.is_none());
        });
    }

    fn enumerate(&mut self) {
        self.devices.clear();
        if let Ok(devices) = raw_devices() {
            for device in devices
                .into_iter()
                .filter(|device| device.dwType == RIM_TYPEHID)
            {
                self.add_device(device.hDevice);
            }
        }
    }

    fn add_device(&mut self, handle: HANDLE) {
        let Ok(info) = device_info(handle) else {
            return;
        };
        if info.dwType != RIM_TYPEHID {
            return;
        }
        // SAFETY: The checked device type selects the HID arm of RID_DEVICE_INFO.
        let hid = unsafe { info.Anonymous.hid };
        if (hid.usUsagePage, hid.usUsage) != (PAGE, USAGE) {
            return;
        }
        let decoder =
            Descriptor::read(handle).and_then(|descriptor| Decoder::new(descriptor, 0, |_| {}));
        match decoder {
            Ok(decoder) => {
                if let Some(scale) = decoder.scale() {
                    self.devices
                        .insert(handle as usize, Device::new(decoder, scale));
                }
            }
            Err(error) => windows::send_log(format!("touchpad_descriptor_unavailable: {error}")),
        }
    }

    fn input(&mut self, input: HRAWINPUT, sink: bool, now: Instant) {
        if !self.registered || windows::is_bypassed() || windows::is_capture_active() {
            return;
        }
        self.refresh(now);
        if !self.registered || self.locked || self.sleeping {
            return;
        }
        let packet = match read_packet(input, &mut self.buffer) {
            Ok(Some(packet)) => packet,
            _ => {
                invalidate();
                update_status(|status| status.last_cancel = Some("invalid_raw_input"));
                return;
            }
        };
        let handle = packet.header.hDevice as usize;
        let Some(batch) = packet.hid else {
            if self.devices.contains_key(&handle) {
                invalidate();
            }
            return;
        };
        let Some(_) = self.devices.get(&handle) else {
            if handle == 0 {
                invalidate();
            }
            return;
        };
        for report in batch.bytes.chunks_exact(batch.report_size) {
            let decoded = self
                .devices
                .get_mut(&handle)
                .expect("known device")
                .decoder
                .decode(report);
            update_status(|status| {
                status.reports += 1;
                status.sink_reports += u64::from(sink);
            });
            let frame = match decoded {
                Ok(Some(frame)) => frame,
                Ok(None) => continue,
                Err(_) => {
                    invalidate();
                    self.devices.get_mut(&handle).expect("known device").cancel(
                        "descriptor_decode_failed",
                        now,
                        self.observe,
                    );
                    continue;
                }
            };
            self.devices
                .get_mut(&handle)
                .expect("known device")
                .occupied = match frame.frame {
                Frame::Empty => false,
                Frame::Single(contact) => contact.tip,
                Frame::Multiple => true,
            };
            let competing = self
                .devices
                .iter()
                .any(|(&key, device)| key != handle && device.occupied);
            if competing {
                invalidate();
                for device in self.devices.values_mut() {
                    device.cancel("multiple_devices", now, self.observe);
                }
            }
            self.devices.get_mut(&handle).expect("known device").frame(
                frame,
                &FrameContext {
                    now,
                    generation: self.generation,
                    parameters: self.parameters,
                    blocked: competing,
                    output: self.output,
                    observe: self.observe,
                },
            );
        }
    }

    fn tick(&mut self, now: Instant) {
        self.refresh(now);
        if !windows::is_bypassed() && !windows::is_capture_active() {
            for device in self.devices.values_mut() {
                device.tick(self.generation, self.output, self.observe, now);
            }
        }
    }
}

fn register(window: HWND, enable: bool) -> io::Result<()> {
    let device = RAWINPUTDEVICE {
        usUsagePage: PAGE,
        usUsage: USAGE,
        dwFlags: if enable {
            RIDEV_INPUTSINK | RIDEV_DEVNOTIFY
        } else {
            RIDEV_REMOVE
        },
        hwndTarget: if enable { window } else { null_mut() },
    };
    // SAFETY: `device` is initialized and has the correct size; removal uses a null target.
    if unsafe { RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

unsafe extern "system" fn window_proc(window: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let handled = WORKER.with(|local| {
        let Ok(mut local) = local.try_borrow_mut() else {
            return None;
        };
        let worker = local.as_mut()?;
        let now = Instant::now();
        match msg {
            WM_INPUT => {
                worker.input(lp as HRAWINPUT, (wp & 0xff) as u32 == RIM_INPUTSINK, now);
                return None;
            }
            WM_TIMER if wp == TIMER => worker.tick(now),
            WM_INPUT_DEVICE_CHANGE if wp == GIDC_ARRIVAL as usize && worker.registered => {
                worker.add_device(lp as HANDLE);
                invalidate();
                worker.refresh(now);
            }
            WM_INPUT_DEVICE_CHANGE if wp == GIDC_REMOVAL as usize => {
                worker.devices.remove(&(lp as usize));
                invalidate();
                worker.refresh(now);
            }
            WM_WTSSESSION_CHANGE
                if wp == WTS_SESSION_LOCK as usize || wp == WTS_SESSION_UNLOCK as usize =>
            {
                worker.locked = wp == WTS_SESSION_LOCK as usize;
                invalidate();
                worker.refresh(now);
            }
            WM_POWERBROADCAST
                if wp == PBT_APMSUSPEND as usize || wp == PBT_APMRESUMEAUTOMATIC as usize =>
            {
                worker.sleeping = wp == PBT_APMSUSPEND as usize;
                invalidate();
                worker.refresh(now);
                return Some(1);
            }
            _ => return None,
        }
        Some(0)
    });
    // SAFETY: Unhandled messages and WM_INPUT cleanup go to the window's default procedure.
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(window, msg, wp, lp) })
}

fn run_window(observe: bool, ready: mpsc::SyncSender<Result<u32, String>>) {
    let class = w!("InputFlowTouchpadInput");
    // SAFETY: These handles, the NUL-terminated class name and WNDCLASSW belong to this process.
    let module = unsafe { GetModuleHandleW(null()) };
    let definition = WNDCLASSW {
        hInstance: module,
        lpszClassName: class,
        lpfnWndProc: Some(window_proc),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&definition) } == 0 {
        let _ = ready.send(Err(format!(
            "touchpad window class: {}",
            io::Error::last_os_error()
        )));
        return;
    }
    // Use a hidden top-level window so the thread also receives power notifications.
    let window = unsafe {
        CreateWindowExW(
            0,
            class,
            class,
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            module,
            null(),
        )
    };
    if window.is_null() {
        let _ = ready.send(Err(format!(
            "touchpad window: {}",
            io::Error::last_os_error()
        )));
        unsafe { UnregisterClassW(class, module) };
        return;
    }
    let session_notifications =
        unsafe { WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION) } != 0;
    let now = Instant::now();
    WORKER.set(Some(Worker {
        window,
        devices: HashMap::new(),
        buffer: Vec::new(),
        registered: false,
        ticking: false,
        observe,
        session_notifications,
        locked: false,
        sleeping: false,
        generation: 0,
        output: false,
        native_off: native_taps_disabled(),
        checked_taps: now,
        parameters: Parameters::default(),
    }));
    let id = unsafe { GetCurrentThreadId() };
    WINDOW_THREAD.store(u64::from(id), Ordering::Release);
    WORKER.with(|state| {
        state
            .borrow_mut()
            .as_mut()
            .expect("initialized worker")
            .refresh(now)
    });
    if ready.send(Ok(id)).is_ok() {
        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
            if message.message == CONFIG_CHANGED && message.hwnd.is_null() {
                WORKER.with(|state| {
                    state
                        .borrow_mut()
                        .as_mut()
                        .expect("initialized worker")
                        .refresh(Instant::now());
                });
            } else {
                unsafe { DispatchMessageW(&message) };
            }
        }
    }
    READY.store(false, Ordering::Release);
    WINDOW_THREAD.store(0, Ordering::Release);
    invalidate();
    WORKER.with(|state| {
        if let Some(worker) = state.borrow_mut().take()
            && worker.registered
        {
            let _ = register(window, false);
        }
    });
    // SAFETY: All resources are owned and cleaned up on the same window thread.
    unsafe {
        KillTimer(window, TIMER);
        if session_notifications {
            WTSUnRegisterSessionNotification(window);
        }
        DestroyWindow(window);
        UnregisterClassW(class, module);
    }
}
