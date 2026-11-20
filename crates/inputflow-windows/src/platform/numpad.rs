use std::collections::VecDeque;
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use inputflow_config::{NumpadConfig, NumpadGuide, PanelLayout, PanelOutput};
use inputflow_engine::NumpadAction;
use inputflow_touchpad::contact::Frame;
use inputflow_touchpad::panel::{Event, Recognizer, Surface};
use windows_sys::Win32::Foundation::{HANDLE, HWND};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND, GUI_INMENUMODE, GUI_INMOVESIZE, GUI_POPUPMENUMODE,
    GUI_SYSTEMMENUMODE, GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo,
    WINEVENT_OUTOFCONTEXT, WM_APP,
};

use super::presentation::overlay::Overlay;
use super::presentation::session::Rect;
use super::{presentation, touchpad, windows};

pub(super) const DIGIT_READY: u32 = WM_APP + 33;
const QUEUE_LIMIT: usize = 16;
static SETTINGS: Mutex<Option<Arc<NumpadConfig>>> = Mutex::new(None);
static SELECT_REQUESTED: Mutex<Option<String>> = Mutex::new(None);
static MODE_NOTICE: AtomicU8 = AtomicU8::new(0);
static CONFIGURED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static START_REQUESTED: AtomicBool = AtomicBool::new(false);
static FAULT: AtomicBool = AtomicBool::new(false);
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
static ACTIVE: AtomicU64 = AtomicU64::new(0);
static INPUT_REVISION: AtomicU64 = AtomicU64::new(1);
static LAST_SUCCESS: AtomicU64 = AtomicU64::new(0);
static PENDING: Mutex<VecDeque<QueuedAction>> = Mutex::new(VecDeque::new());
static STATUS: Mutex<NumpadStatus> = Mutex::new(NumpadStatus::new());

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumpadStatus {
    pub configured: bool,
    pub phase: &'static str,
    pub reason: &'static str,
    pub reports: u64,
    pub confirmed: u64,
    pub cancelled: u64,
    pub requested_inputs: u64,
    pub inserted_inputs: u64,
    pub last_cancel: Option<&'static str>,
    pub active_layout: Option<String>,
    pub size_um: Option<[u32; 2]>,
}

impl NumpadStatus {
    const fn new() -> Self {
        Self {
            configured: false,
            phase: "disabled",
            reason: "disabled",
            reports: 0,
            confirmed: 0,
            cancelled: 0,
            requested_inputs: 0,
            inserted_inputs: 0,
            last_cancel: None,
            active_layout: None,
            size_um: None,
        }
    }
}

impl Default for NumpadStatus {
    fn default() -> Self {
        Self::new()
    }
}

pub fn status() -> NumpadStatus {
    STATUS
        .lock()
        .map_or(NumpadStatus::new(), |status| status.clone())
}

fn update_status(update: impl FnOnce(&mut NumpadStatus)) {
    if let Ok(mut status) = STATUS.lock() {
        update(&mut status);
    }
}

pub fn configure(experimental: bool, config: &NumpadConfig) {
    let enabled = experimental && config.enabled;
    if let Ok(mut settings) = SETTINGS.lock() {
        *settings = Some(Arc::new(config.clone()));
    }
    CONFIGURED.store(enabled, Ordering::Release);
    FAULT.store(false, Ordering::Release);
    request_stop();
    if let Ok(mut pending) = PENDING.lock() {
        pending.clear();
    }
}

pub(crate) fn is_configured() -> bool {
    CONFIGURED.load(Ordering::Acquire)
}

pub(crate) fn is_ready() -> bool {
    READY.load(Ordering::Acquire) && !presentation::is_active() && !windows::is_bypassed()
}

pub(crate) fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire) != 0
}

pub(crate) fn request_stop() {
    STOP.store(true, Ordering::Release);
    READY.store(false, Ordering::Release);
    START_REQUESTED.store(false, Ordering::Release);
    if let Ok(mut selection) = SELECT_REQUESTED.lock() {
        selection.take();
    }
    if ACTIVE.swap(0, Ordering::AcqRel) != 0 {
        touchpad::invalidate();
    }
    touchpad::wake_worker();
}

/// Called from the hook before passing an event that can change keyboard focus or selection.
pub(crate) fn input_changed() {
    if is_active() {
        INPUT_REVISION.fetch_add(1, Ordering::AcqRel);
    }
}

pub(crate) fn enqueue(action: NumpadAction) {
    match action {
        NumpadAction::Stop => request_stop(),
        NumpadAction::Start if presentation::is_active() => mode_notice(1),
        NumpadAction::Start if is_ready() && !is_active() => {
            START_REQUESTED.store(true, Ordering::Release);
            touchpad::wake_worker();
        }
        NumpadAction::Select(id) if is_active() => {
            if let Ok(mut selection) = SELECT_REQUESTED.lock() {
                *selection = Some(id);
            }
            touchpad::wake_worker();
        }
        _ => {}
    }
}

pub(super) fn mode_notice(mode: u8) {
    MODE_NOTICE.store(mode, Ordering::Release);
    touchpad::wake_worker();
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Focus {
    foreground: usize,
    control: usize,
}

impl Focus {
    fn current() -> Option<Self> {
        let mut info = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: Win32 writes to a sized, initialized GUITHREADINFO. The handles are compared as values.
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_null()
            || unsafe { GetGUIThreadInfo(0, &mut info) } == 0
            || info.hwndActive != foreground
            || info.hwndFocus.is_null()
            || info.flags
                & (GUI_INMENUMODE | GUI_INMOVESIZE | GUI_POPUPMENUMODE | GUI_SYSTEMMENUMODE)
                != 0
        {
            return None;
        }
        Some(Self {
            foreground: foreground as usize,
            control: info.hwndFocus as usize,
        })
    }
}

struct FocusWatch([HWINEVENTHOOK; 2]);

impl FocusWatch {
    fn new() -> std::io::Result<Self> {
        let mut watch = Self([null_mut(); 2]);
        for (slot, event) in watch
            .0
            .iter_mut()
            .zip([EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_FOCUS])
        {
            // SAFETY: Out-of-context callbacks run on this worker's message loop. The callback uses no borrowed data.
            *slot = unsafe {
                SetWinEventHook(
                    event,
                    event,
                    null_mut(),
                    Some(focus_changed),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if slot.is_null() {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(watch)
    }
}

impl Drop for FocusWatch {
    fn drop(&mut self) {
        for handle in self.0.into_iter().filter(|handle| !handle.is_null()) {
            // SAFETY: The worker that installed these hooks owns and releases them.
            unsafe { UnhookWinEvent(handle) };
        }
    }
}

unsafe extern "system" fn focus_changed(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    INPUT_REVISION.fetch_add(1, Ordering::AcqRel);
}

fn modifiers_held() -> bool {
    // SAFETY: Each value is a Windows virtual key. Only the high bit (down state) is read.
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) < 0 })
}

#[derive(Clone, Copy)]
struct ContactGuard {
    focus: Focus,
    device: usize,
    generation: u64,
    input_revision: u64,
}

impl ContactGuard {
    fn capture(device: usize) -> Option<Self> {
        let generation = touchpad::generation();
        let input_revision = INPUT_REVISION.load(Ordering::Acquire);
        let focus = Focus::current()?;
        Some(Self {
            focus,
            device,
            generation,
            input_revision,
        })
    }

    fn changed(self) -> Option<&'static str> {
        if self.generation != touchpad::generation() {
            Some("state_changed")
        } else if Focus::current() != Some(self.focus) {
            Some("focus_changed")
        } else if modifiers_held() {
            Some("modifiers_held")
        } else if self.input_revision != INPUT_REVISION.load(Ordering::Acquire) {
            Some("other_input")
        } else {
            None
        }
    }
}

struct QueuedAction {
    session: u64,
    guard: ContactGuard,
    output: PanelOutput,
}

impl QueuedAction {
    fn can_send(&self) -> bool {
        ACTIVE.load(Ordering::Acquire) == self.session
            && is_ready()
            && !windows::is_capture_active()
            && !touchpad::physical_mouse_pressed()
            && touchpad::native_taps_disabled()
            && inputflow_touchpad::raw::device_info(self.guard.device as HANDLE).is_ok()
            && self.guard.changed().is_none()
    }
}

pub(super) fn process_digits() {
    let action = PENDING.lock().ok().and_then(|mut queue| queue.pop_front());
    let Some(action) = action else { return };
    if let Some(result) = windows::send_touchpad_action(&action.output, || action.can_send()) {
        update_status(|status| {
            status.requested_inputs += u64::from(result.requested);
            status.inserted_inputs += u64::from(result.inserted);
        });
        if result.inserted == result.requested {
            LAST_SUCCESS.store(action.session, Ordering::Release);
        } else {
            FAULT.store(true, Ordering::Release);
            request_stop();
        }
    } else {
        cancelled(action.guard.changed().unwrap_or("output_guard"));
    }
}

fn queue_action(action: QueuedAction) {
    if let Ok(mut pending) = PENDING.lock() {
        if pending.len() >= QUEUE_LIMIT {
            cancelled("output_queue_full");
            return;
        }
        pending.push_back(action);
        if !windows::wake_touchpad_output(DIGIT_READY) {
            pending.pop_back();
            cancelled("hook_unavailable");
        }
    }
}

fn cancelled(reason: &'static str) {
    update_status(|status| {
        status.cancelled += 1;
        status.last_cancel = Some(reason);
    });
}

#[derive(Default)]
struct Contact {
    recognizer: Recognizer,
    guard: Option<ContactGuard>,
    waiting_for_clear: bool,
    device: Option<usize>,
}

impl Contact {
    fn interrupt(&mut self, reason: &'static str, occupied: bool) -> Option<Event> {
        self.guard = None;
        self.waiting_for_clear = occupied;
        let event = self.recognizer.interrupt(reason);
        if !occupied {
            self.recognizer = Recognizer::default();
        }
        event
    }
}

struct Guide {
    overlay: Overlay,
    policy: NumpadGuide,
    start: Instant,
    drawn: Option<usize>,
}

impl Guide {
    fn new(
        policy: NumpadGuide,
        layout: &PanelLayout,
        surface: &Surface,
        size: Option<[u32; 2]>,
        now: Instant,
    ) -> std::io::Result<Option<Self>> {
        if policy == NumpadGuide::Never {
            return Ok(None);
        }
        let mut overlay = corner_overlay(
            300,
            size.map_or(180, |[w, h]| {
                (300.0 * f64::from(h) / f64::from(w)).clamp(76.0, 420.0) as i32
            }),
        )?;
        overlay.panel(&layout.geometry, surface, None)?;
        Ok(Some(Self {
            overlay,
            policy,
            start: now,
            drawn: None,
        }))
    }

    fn expired(&self, token: u64, now: Instant) -> bool {
        match self.policy {
            NumpadGuide::Always => false,
            NumpadGuide::TwoSeconds => now.duration_since(self.start) >= Duration::from_secs(2),
            NumpadGuide::UntilFirstInput => LAST_SUCCESS.load(Ordering::Acquire) == token,
            NumpadGuide::Never => true,
        }
    }
}

fn corner_overlay(width: i32, height: i32) -> std::io::Result<Overlay> {
    // SAFETY: The foreground HWND is used immediately by Win32 and never dereferenced here.
    let monitor = unsafe { MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: GetMonitorInfoW writes to the initialized MONITORINFO.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let right = info.rcWork.right - 20;
    let bottom = info.rcWork.bottom - 20;
    Overlay::new(Rect {
        left: right - width,
        top: bottom - height,
        right,
        bottom,
    })
}

struct Session {
    id: u64,
    layout: PanelLayout,
    surface: Surface,
    contact: Contact,
    guide: Option<Guide>,
    _focus_watch: Option<FocusWatch>,
}

pub(super) struct Controller {
    observe: bool,
    session: Option<Session>,
    session_fault: Option<&'static str>,
    notice: Option<(Overlay, Instant)>,
}

pub(super) struct Report {
    pub device: usize,
    pub frame: Frame,
    pub physical_button: Option<bool>,
    pub bounds: [(i32, i32); 2],
    pub occupied: bool,
    pub competing: bool,
    pub now: Instant,
}

impl Controller {
    pub fn new(observe: bool) -> Self {
        update_status(|status| *status = NumpadStatus::new());
        Self {
            observe,
            session: None,
            session_fault: None,
            notice: None,
        }
    }

    fn start(
        &mut self,
        layout: PanelLayout,
        policy: NumpadGuide,
        size: Option<[u32; 2]>,
        occupied: bool,
        now: Instant,
    ) {
        let focus_watch = match FocusWatch::new() {
            Ok(watch) => Some(watch),
            Err(_) if self.observe => None,
            Err(error) => {
                self.session_fault = Some("focus_watch_unavailable");
                windows::send_log(format!("numpad focus watch: {error}"));
                return;
            }
        };
        let surface = Surface::compile(&layout.geometry);
        let guide = match Guide::new(policy, &layout, &surface, size, now) {
            Ok(guide) => guide,
            Err(error) => {
                self.session_fault = Some("overlay_failed");
                windows::send_log(format!("numpad guide: {error}"));
                return;
            }
        };
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        self.session = Some(Session {
            id,
            layout,
            surface,
            guide,
            _focus_watch: focus_watch,
            contact: Contact {
                waiting_for_clear: occupied,
                ..Contact::default()
            },
        });
        if !self.observe {
            ACTIVE.store(id, Ordering::Release);
            touchpad::invalidate();
        }
    }

    fn stop(&mut self) {
        if self.session.take().is_some() && ACTIVE.swap(0, Ordering::AcqRel) != 0 {
            touchpad::invalidate();
        }
    }

    pub fn reset_topology(&mut self) {
        request_stop();
        self.stop();
    }

    pub fn refresh(
        &mut self,
        blocked: Option<&'static str>,
        occupied: bool,
        size: Option<[u32; 2]>,
        now: Instant,
    ) {
        if self.notice.as_ref().is_some_and(|(_, until)| now >= *until) {
            self.notice = None;
        }
        let notice = MODE_NOTICE.swap(0, Ordering::AcqRel);
        if notice != 0 {
            let message = if notice == 1 {
                "Exit the laser pointer first / 请先退出演示激光笔 (Esc)"
            } else {
                "Exit the virtual panel first / 请先退出虚拟操作面板 (Esc)"
            };
            if let Ok(mut overlay) = corner_overlay(380, 52)
                && overlay.message(message).is_ok()
            {
                self.notice = Some((overlay, now + Duration::from_secs(2)));
            }
        }
        update_status(|status| status.size_um = size);
        if !self.observe
            && !is_configured()
            && self.session.is_none()
            && !STOP.load(Ordering::Acquire)
        {
            return;
        }
        let settings = SETTINGS.lock().ok().and_then(|settings| settings.clone());
        let configured = is_configured();
        let target = settings.as_ref().and_then(|config| {
            config
                .layouts
                .iter()
                .find(|layout| Some(&layout.id) == config.default_layout.as_ref())
        });
        let stopped = STOP.swap(false, Ordering::AcqRel);
        if stopped {
            self.stop();
            self.session_fault = None;
        }
        let reason = if self.observe {
            blocked.unwrap_or("observe_only")
        } else if !configured {
            "disabled"
        } else if FAULT.load(Ordering::Acquire) {
            "output_fault"
        } else if let Some(reason) = blocked {
            reason
        } else if target.is_none() {
            "no_layout"
        } else {
            self.session_fault.unwrap_or("ready")
        };
        let ready = reason == "ready";
        if !ready && reason != "observe_only" {
            self.stop();
        }
        let start_requested = START_REQUESTED.swap(false, Ordering::AcqRel);
        let selection = SELECT_REQUESTED
            .lock()
            .ok()
            .and_then(|mut request| request.take());
        if !stopped && !STOP.load(Ordering::Acquire) {
            if self.session.is_none() && self.observe && blocked.is_none() {
                self.start(
                    PanelLayout::classic(),
                    NumpadGuide::Never,
                    size,
                    occupied,
                    now,
                );
            } else if ready && let Some(config) = settings.as_ref() {
                let next = match (self.session.as_ref(), selection) {
                    (None, _) if start_requested => target,
                    (Some(session), Some(id)) if session.layout.id != id => {
                        config.layouts.iter().find(|layout| layout.id == id)
                    }
                    _ => None,
                };
                if let Some(layout) = next {
                    self.stop();
                    if let Ok(mut pending) = PENDING.lock() {
                        pending.clear();
                    }
                    self.start(layout.clone(), config.guide, size, occupied, now);
                }
            }
        }
        let reason = self.session_fault.unwrap_or(reason);
        READY.store(
            ready && self.session_fault.is_none() && !STOP.load(Ordering::Acquire),
            Ordering::Release,
        );
        let phase = match (self.session.is_some(), reason) {
            (true, "observe_only") => "observing",
            (true, _) => "active",
            (false, "ready") => "ready",
            (false, "disabled") => "disabled",
            _ => "unavailable",
        };
        let reason = if phase == "active" && Focus::current().is_none() {
            "focus_unavailable"
        } else {
            reason
        };
        update_status(|status| {
            status.configured = configured;
            status.phase = phase;
            status.reason = reason;
            status.active_layout = self
                .session
                .as_ref()
                .map(|session| session.layout.id.clone());
        });
    }

    pub fn frame(&mut self, report: Report) {
        let Report {
            device: handle,
            frame,
            physical_button,
            bounds,
            occupied,
            competing,
            now,
        } = report;
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !self.observe && ACTIVE.load(Ordering::Acquire) != session.id {
            return;
        }
        update_status(|status| status.reports += 1);
        let contact = &mut session.contact;
        let event = if let Some(reason) = if competing {
            Some("multiple_devices")
        } else if physical_button == Some(true) || touchpad::physical_mouse_pressed() {
            Some("physical_button")
        } else if !self.observe && physical_button.is_none() {
            Some("unknown_button_state")
        } else {
            contact.guard.and_then(ContactGuard::changed)
        } {
            contact.interrupt(reason, occupied)
        } else if contact.waiting_for_clear {
            if !occupied {
                contact.interrupt("cleared", false);
            }
            None
        } else if matches!(frame, Frame::Single(sample) if sample.tip) && contact.guard.is_none() {
            if contact
                .device
                .replace(handle)
                .is_some_and(|old| old != handle)
            {
                input_changed();
            }
            contact.guard = ContactGuard::capture(handle);
            if contact.guard.is_none() || modifiers_held() {
                contact.interrupt("focus_or_modifiers", occupied);
                Some(Event::Cancelled("focus_or_modifiers"))
            } else {
                contact.recognizer.observe(
                    frame,
                    bounds,
                    &session.layout.geometry,
                    &session.surface,
                    now,
                )
            }
        } else if contact.device.is_none_or(|device| device == handle) {
            contact.recognizer.observe(
                frame,
                bounds,
                &session.layout.geometry,
                &session.surface,
                now,
            )
        } else {
            None
        };
        let guard = if occupied {
            contact.guard
        } else {
            contact.guard.take()
        };
        if let Some(event) = event {
            Self::record(self.observe, session, guard, event);
        }
    }

    pub fn interrupt(&mut self, reason: &'static str, occupied: bool) {
        if let Some(session) = &mut self.session
            && let Some(event) = session.contact.interrupt(reason, occupied)
        {
            Self::record(self.observe, session, None, event);
        }
    }

    fn record(observe: bool, session: &Session, guard: Option<ContactGuard>, event: Event) {
        match event {
            Event::Cancelled(reason) => cancelled(reason),
            Event::Confirmed(value) => {
                update_status(|status| status.confirmed += 1);
                if !observe
                    && let Some(guard) = guard
                    && let Some(output) = session.layout.geometry.regions[value]
                        .action
                        .as_ref()
                        .and_then(|action| action.resolve().ok())
                {
                    queue_action(QueuedAction {
                        session: session.id,
                        guard,
                        output,
                    });
                }
            }
            Event::Highlight(_) => {}
        }
        if observe {
            windows::send_log(format!(
                "numpad_observe event={event:?} focus={:?}",
                Focus::current().map(|target| target.control)
            ));
        }
    }

    pub fn tick(&mut self, occupied: bool, now: Instant) {
        let Some(session) = &mut self.session else {
            return;
        };
        if let Some(reason) = session.contact.guard.and_then(ContactGuard::changed)
            && let Some(event) = session.contact.interrupt(reason, occupied)
        {
            Self::record(self.observe, session, None, event);
        }
        if let Some(guide) = &mut session.guide {
            let highlight = session.contact.recognizer.highlight();
            if guide.expired(session.id, now) {
                session.guide = None;
            } else if guide.drawn != highlight {
                match guide
                    .overlay
                    .panel(&session.layout.geometry, &session.surface, highlight)
                {
                    Ok(()) => guide.drawn = highlight,
                    Err(error) => {
                        self.stop();
                        self.session_fault = Some("overlay_failed");
                        READY.store(false, Ordering::Release);
                        windows::send_log(format!("numpad guide redraw: {error}"));
                    }
                }
            }
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.stop();
        READY.store(false, Ordering::Release);
    }
}
