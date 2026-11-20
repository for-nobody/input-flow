//! Agent-owned presentation state. Only the touchpad message thread owns windows.
pub(super) mod overlay;
pub(super) mod session;

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use inputflow_config::PresentationConfig;
use inputflow_engine::PresentationAction;
use inputflow_touchpad::contact::Frame;
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetPhysicalCursorPos, SetPhysicalCursorPos};

use self::overlay::{Display, Overlay};
use self::session::{Brush, Rect, Session};

static CONFIG: Mutex<Option<(bool, PresentationConfig)>> = Mutex::new(None);
static REVISION: AtomicU64 = AtomicU64::new(1);
static CONFIGURED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static POINTING: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static IDENTIFY: AtomicBool = AtomicBool::new(false);
static ACTIONS: Mutex<VecDeque<PresentationAction>> = Mutex::new(VecDeque::new());
static STATUS: Mutex<Option<PresentationStatus>> = Mutex::new(None);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PresentationStatus {
    pub enabled: bool,
    pub can_start: bool,
    pub phase: &'static str,
    pub reason: &'static str,
    pub preset: Option<String>,
    pub display: Option<String>,
    pub pointer: Option<(i32, i32)>,
    pub mouse_position: Option<(i32, i32)>,
    pub system_cursor: Option<(i32, i32)>,
    pub segments: usize,
    pub frames: u64,
    pub peak_render_us: u64,
    pub displays: Vec<DisplayStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayStatus {
    pub id: String,
    pub primary: bool,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

pub fn status() -> PresentationStatus {
    let mut status = STATUS
        .lock()
        .ok()
        .and_then(|status| status.clone())
        .unwrap_or_default();
    if status.enabled {
        status.system_cursor = physical_cursor();
    }
    status
}

pub fn configure(experimental: bool, config: PresentationConfig) {
    request_stop();
    CONFIGURED.store(experimental && config.enabled, Ordering::Release);
    if let Ok(mut saved) = CONFIG.lock() {
        *saved = Some((experimental, config));
        REVISION.fetch_add(1, Ordering::Release);
    }
    super::touchpad::wake_worker();
}

pub(crate) fn is_configured() -> bool {
    CONFIGURED.load(Ordering::Acquire)
}
pub(crate) fn is_ready() -> bool {
    READY.load(Ordering::Acquire)
}
pub(crate) fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}
pub(crate) fn is_pointing() -> bool {
    POINTING.load(Ordering::Acquire)
}

pub(crate) fn request_stop() {
    ACTIVE.store(false, Ordering::Release);
    READY.store(false, Ordering::Release);
    STOP.store(true, Ordering::Release);
    super::touchpad::wake_worker();
}

pub(crate) fn enqueue(action: PresentationAction) {
    if action == PresentationAction::Start && super::numpad::is_active() {
        super::numpad::mode_notice(2);
        return;
    }
    if action == PresentationAction::Stop {
        request_stop();
        return;
    }
    if is_ready()
        && let Ok(mut actions) = ACTIONS.lock()
    {
        if actions.len() == 16 {
            request_stop();
        } else {
            actions.push_back(action);
        }
    }
    super::touchpad::wake_worker();
}

pub fn identify_displays() {
    IDENTIFY.store(true, Ordering::Release);
    super::touchpad::wake_worker();
}

struct Active {
    model: Session,
    display: Display,
    overlay: Overlay,
}

pub(super) struct Presenter {
    revision: u64,
    config: PresentationConfig,
    displays: Vec<Display>,
    active: Option<Active>,
    identities: Vec<Overlay>,
    identities_until: Option<Instant>,
    frames: u64,
    peak_render_us: u64,
    fault: Option<&'static str>,
    device_removed: bool,
    last_error: Option<&'static str>,
}

impl Presenter {
    pub fn new() -> Self {
        Self {
            revision: 0,
            config: PresentationConfig::default(),
            displays: overlay::displays(),
            active: None,
            identities: Vec::new(),
            identities_until: None,
            frames: 0,
            peak_render_us: 0,
            fault: None,
            device_removed: false,
            last_error: None,
        }
    }

    pub fn needs_timer(&self) -> bool {
        self.identities_until.is_some()
    }

    pub fn topology_changed(&mut self) {
        self.displays = overlay::displays();
        if self
            .active
            .as_ref()
            .is_some_and(|active| !self.displays.contains(&active.display))
        {
            self.stop();
            self.last_error = Some("display_changed");
        }
        self.identities.clear();
        self.identities_until = None;
    }

    pub fn device_removed(&mut self, device: usize) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.model.owner.is_none_or(|owner| owner == device))
        {
            self.stop();
            self.device_removed = true;
            self.last_error = Some("device_removed");
        }
    }

    pub fn cancel_contact(&mut self, occupied: bool) {
        if let Some(active) = &mut self.active {
            active.model.interrupt(occupied);
        }
    }

    fn selected_display(&self) -> Option<&Display> {
        match (self.config.display.as_deref(), self.displays.as_slice()) {
            (_, [only]) => Some(only),
            (Some(id), _) => self.displays.iter().find(|display| display.id == id),
            (None, _) => self.displays.first(),
        }
    }

    pub fn refresh(&mut self, device_available: bool, occupied: bool, blocked: bool, now: Instant) {
        let revision = REVISION.load(Ordering::Acquire);
        if revision != self.revision
            && let Ok(saved) = CONFIG.lock()
            && let Some((_, config)) = saved.as_ref()
        {
            self.stop();
            self.config.clone_from(config);
            self.revision = revision;
            self.fault = None;
            self.last_error = None;
        }
        let stop_requested = STOP.swap(false, Ordering::AcqRel);
        let ready = is_configured()
            && device_available
            && !blocked
            && self.selected_display().is_some()
            && self.fault.is_none();
        if !ready || stop_requested {
            self.stop();
        }
        let actions = ACTIONS
            .lock()
            .map_or_else(|_| VecDeque::new(), |mut queue| std::mem::take(&mut *queue));
        if ready && !stop_requested {
            for action in actions {
                self.apply(action, occupied);
            }
        }
        let ready = ready && self.fault.is_none() && !STOP.load(Ordering::Acquire);
        READY.store(ready, Ordering::Release);
        self.update_identity(now);
        self.publish(ready, blocked, device_available);
    }

    fn apply(&mut self, action: PresentationAction, occupied: bool) {
        if action == PresentationAction::Start {
            if self.active.is_some() {
                return;
            }
            let Some(display) = self.selected_display().cloned() else {
                return;
            };
            let Some(preset) = self
                .config
                .presets
                .iter()
                .find(|preset| preset.id == self.config.default_preset)
            else {
                return;
            };
            let Some(mouse) = physical_cursor() else {
                self.fault = Some("cursor_unavailable");
                return;
            };
            let bounds = display.bounds;
            let Some(model) = Session::new(
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                self.config.movement,
                preset,
                mouse,
                occupied,
            ) else {
                return;
            };
            match Overlay::new(bounds) {
                Ok(overlay) => {
                    self.active = Some(Active {
                        model,
                        display,
                        overlay,
                    });
                    self.frames = 0;
                    self.peak_render_us = 0;
                    self.device_removed = false;
                    self.last_error = None;
                    ACTIVE.store(true, Ordering::Release);
                    POINTING.store(true, Ordering::Release);
                    super::touchpad::invalidate();
                    super::windows::send_log("presentation started".to_string());
                }
                Err(error) => {
                    self.fault = Some("overlay_failed");
                    super::windows::send_log(format!("presentation overlay: {error}"));
                }
            }
            return;
        }
        let Some(active) = &mut self.active else {
            return;
        };
        match action {
            PresentationAction::Start => unreachable!(),
            PresentationAction::Stop => self.stop(),
            PresentationAction::Clear => {
                active.model.clear(occupied);
                super::windows::send_log("presentation ink cleared".to_string());
            }
            PresentationAction::ToggleMouse => {
                let mouse = physical_cursor().unwrap_or(active.model.mouse_position);
                if let Some(position) = active.model.toggle_mouse(mouse, occupied) {
                    restore_cursor(position);
                }
                POINTING.store(!active.model.temporary, Ordering::Release);
                super::touchpad::invalidate();
                super::windows::send_log(format!(
                    "presentation temporary_mouse={} segments={}",
                    active.model.temporary,
                    active.model.ink.len()
                ));
            }
            PresentationAction::Select(id) => {
                if let Some(preset) = self.config.presets.iter().find(|preset| preset.id == id) {
                    let mouse = physical_cursor().unwrap_or(active.model.mouse_position);
                    active.model.select(preset, mouse, occupied);
                    POINTING.store(true, Ordering::Release);
                    super::touchpad::invalidate();
                }
            }
        }
    }

    pub fn frame(
        &mut self,
        device: usize,
        frame: Frame,
        logical: [(i32, i32); 2],
        blocked: bool,
        now: Instant,
    ) {
        if let Some(active) = &mut self.active {
            active.model.frame(device, frame, logical, blocked, now);
        }
    }

    pub fn render(&mut self, now: Instant) {
        let Some(active) = &mut self.active else {
            return;
        };
        active.model.expire(now);
        if let Some(dirty) = active.model.take_dirty() {
            let pointer =
                (!active.model.temporary).then(|| (active.model.pointer(), active.model.brush));
            let started = Instant::now();
            if let Err(error) = active.overlay.render(dirty, &active.model.ink, pointer) {
                self.stop();
                READY.store(false, Ordering::Release);
                self.fault = Some("overlay_failed");
                super::windows::send_log(format!("presentation render: {error}"));
                return;
            }
            self.frames += 1;
            self.peak_render_us = self
                .peak_render_us
                .max(started.elapsed().as_micros() as u64);
        }
    }

    pub fn stop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
        POINTING.store(false, Ordering::Release);
        if let Some(active) = self.active.take() {
            if !active.model.temporary {
                restore_cursor(active.model.mouse_position);
            }
            super::touchpad::invalidate();
            super::windows::send_log(format!(
                "presentation ended: frames={} peak_render_us={} segments={}",
                self.frames,
                self.peak_render_us,
                active.model.ink.len()
            ));
        }
    }

    fn update_identity(&mut self, now: Instant) {
        if IDENTIFY.swap(false, Ordering::AcqRel) {
            self.identities.clear();
            for (index, display) in self.displays.iter().enumerate() {
                let Rect {
                    left,
                    top,
                    right,
                    bottom,
                } = display.bounds;
                let bounds = Rect {
                    left: left + 24,
                    top: top + 24,
                    right: (left + 124).min(right),
                    bottom: (top + 100).min(bottom),
                };
                if let Ok(mut overlay) = Overlay::new(bounds)
                    && overlay.identify(index + 1).is_ok()
                {
                    self.identities.push(overlay);
                }
            }
            self.identities_until = Some(now + Duration::from_secs(2));
        }
        if self.identities_until.is_some_and(|until| now >= until) {
            self.identities.clear();
            self.identities_until = None;
        }
    }

    fn publish(&self, ready: bool, blocked: bool, device_available: bool) {
        let Ok(mut status) = STATUS.lock() else {
            return;
        };
        let value = status.get_or_insert_default();
        if value.displays.len() != self.displays.len()
            || value.displays.iter().zip(&self.displays).any(|(old, new)| {
                old.id != new.id
                    || (old.left, old.top, old.width, old.height, old.primary) != dimensions(new)
            })
        {
            value.displays = self
                .displays
                .iter()
                .map(|display| {
                    let (left, top, width, height, primary) = dimensions(display);
                    DisplayStatus {
                        id: display.id.clone(),
                        primary,
                        left,
                        top,
                        width,
                        height,
                    }
                })
                .collect();
        }
        value.enabled = is_configured();
        value.can_start = ready;
        value.reason = if let Some(reason) = self.fault {
            reason
        } else if !value.enabled {
            "disabled"
        } else if super::numpad::is_active() {
            "numpad_active"
        } else if blocked {
            "paused"
        } else if !device_available {
            "device_unavailable"
        } else if self.selected_display().is_none() {
            "display_unavailable"
        } else if let Some(reason) = self.last_error {
            reason
        } else {
            "ready"
        };
        value.phase = if ready { "ready" } else { "unavailable" };
        value.frames = self.frames;
        value.peak_render_us = self.peak_render_us;
        value.pointer = None;
        value.mouse_position = None;
        value.segments = 0;
        if let Some(active) = self.active.as_ref() {
            let model = &active.model;
            value.phase = if model.temporary {
                "mouse"
            } else if matches!(model.brush, Brush::Ink { .. }) {
                "drawing"
            } else {
                "pointing"
            };
            let (x, y) = model.pointer();
            value.pointer = Some((
                active.display.bounds.left + x,
                active.display.bounds.top + y,
            ));
            value.mouse_position = Some(model.mouse_position);
            if value.preset.as_ref() != Some(&model.preset_id) {
                value.preset = Some(model.preset_id.clone());
            }
            if value.display.as_ref() != Some(&active.display.id) {
                value.display = Some(active.display.id.clone());
            }
            value.segments = model.ink.len();
            if model.capacity_reached {
                value.reason = "ink_capacity";
            }
        } else {
            value.preset = None;
            value.display = None;
        }
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        self.stop();
        READY.store(false, Ordering::Release);
    }
}

fn dimensions(display: &Display) -> (i32, i32, i32, i32, bool) {
    let bounds = display.bounds;
    (
        bounds.left,
        bounds.top,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        display.primary,
    )
}

fn physical_cursor() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    // SAFETY: The output points to an initialized Win32 coordinate structure.
    (unsafe { GetPhysicalCursorPos(&mut point) } != 0).then_some((point.x, point.y))
}

fn restore_cursor((x, y): (i32, i32)) {
    // SAFETY: Coordinates are the physical mouse location saved for this session.
    if unsafe { SetPhysicalCursorPos(x, y) } == 0 {
        super::windows::send_log("presentation cursor restoration failed".into());
    }
}
