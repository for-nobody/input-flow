use std::collections::{HashMap, VecDeque};
use std::io;
use std::mem::size_of;
use std::ptr::null_mut;
use std::time::Instant;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::{PtInRect, ScreenToClient};
use windows_sys::Win32::UI::Controls::{EM_GETSEL, WM_MOUSELEAVE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_LBUTTON, VK_RBUTTON,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::contact::{Frame, Transition};
use crate::gesture::{Decision, Event, Parameters, Recognizer, Scale};
use crate::hid::Decoded;
use crate::output::{self, Button, Mode, Plan};

pub(super) const SHOW_STATUS: u32 = WM_APP + 2;

#[derive(Default)]
struct TargetCounts {
    left_down: u32,
    left_up: u32,
    left_double: u32,
    right_down: u32,
    right_up: u32,
    right_double: u32,
    wheel: u32,
    drags: u32,
}

pub(super) struct Stage2 {
    parameters: Parameters,
    recognizer: Recognizer,
    window: HWND,
    target: HWND,
    status: HWND,
    timer: Option<usize>,
    sample: Option<(&'static str, Instant)>,
    button: Option<bool>,
    counts: TargetCounts,
    drag_motion: bool,
    message: String,
    mode: Mode,
    button_verified: bool,
    pending: VecDeque<Plan>,
    receipts: HashMap<usize, Instant>,
}

impl Stage2 {
    pub(super) fn new(parameters: Parameters) -> Self {
        Self {
            parameters,
            recognizer: Recognizer::default(),
            window: null_mut(),
            target: null_mut(),
            status: null_mut(),
            timer: None,
            sample: None,
            button: None,
            counts: TargetCounts::default(),
            drag_motion: false,
            message: "Ready".into(),
            mode: Mode::Observe,
            button_verified: false,
            pending: VecDeque::new(),
            receipts: HashMap::new(),
        }
    }

    pub(super) fn attach(&mut self, window: HWND, target: HWND, status: HWND) {
        self.window = window;
        self.target = target;
        self.status = status;
    }

    pub(super) fn start(&mut self, name: &'static str, now: Instant) {
        self.sample = Some((name, now));
        self.counts = TargetCounts::default();
        self.drag_motion = false;
        log!(
            "stage2_start sample={name} mode={} target_inside={} short_ms={} hold_ms={} movement_mm={:.3} movement_normalized={:.6} target_double_clicks={}",
            self.mode.label(),
            self.target_hit(),
            self.parameters.short.as_millis(),
            self.parameters.hold.as_millis(),
            self.parameters.movement_mm,
            self.parameters.movement,
            // SAFETY: target is the live EDIT child owned by this window.
            unsafe { GetClassLongPtrW(self.target, GCL_STYLE) } & CS_DBLCLKS as usize != 0
        );
        self.show(format!("Recording {name}"));
    }

    pub(super) fn active(&self) -> bool {
        self.sample.is_some()
    }

    pub(super) fn toggle_output(&mut self, mode: Mode, clear: bool, now: Instant) {
        if self.mode != Mode::Observe {
            self.disarm("manual_stop", now);
            return;
        }
        let inside = self.target_hit();
        if !self.button_verified || self.button != Some(false) || !inside || !clear {
            log!(
                "stage2_output_blocked reason=arm_guard verified={} target_inside={inside} contacts_clear={clear} button={:?}",
                self.button_verified,
                self.button
            );
            self.show(
                "Output blocked. Test a physical press first, lift, and aim inside the box.".into(),
            );
            return;
        }
        let mut tracking = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.target,
            dwHoverTime: 0,
        };
        // SAFETY: Request only WM_MOUSELEAVE for the experiment's own child, currently under the pointer.
        if unsafe { TrackMouseEvent(&mut tracking) } == 0 {
            log!("stage2_output_blocked reason=mouse_leave_registration");
            return;
        }
        self.interrupt("output_enabled", now);
        self.mode = mode;
        log!("stage2_mode={} reason=explicit_key", self.mode.label());
        self.show(format!(
            "{} enabled inside the test box. F6/F7 stops output.",
            self.mode.label()
        ));
    }

    pub(super) fn disarm(&mut self, reason: &'static str, now: Instant) {
        if self.mode != Mode::Observe {
            self.mode = Mode::Observe;
            self.pending.clear();
            self.interrupt(reason, now);
            log!("stage2_mode=observe reason={reason}");
            self.show(format!("Output stopped ({reason})"));
        }
    }

    pub(super) fn guard_target(&mut self, now: Instant) {
        if self.mode != Mode::Observe && !self.target_hit() {
            self.disarm("target_left", now);
        }
    }

    fn target_hit(&self) -> bool {
        // SAFETY: All APIs are queries using our live target and initialized coordinate buffers.
        unsafe {
            let mut point = POINT::default();
            let mut rect = RECT::default();
            GetForegroundWindow() == self.window
                && !self.target.is_null()
                && GetCursorPos(&mut point) != 0
                && WindowFromPoint(point) == self.target
                && ScreenToClient(self.target, &mut point) != 0
                && GetClientRect(self.target, &mut rect) != 0
                && PtInRect(&rect, point) != 0
        }
    }

    pub(super) fn next_output(&mut self, now: Instant) -> Option<Plan> {
        if self.pending.is_empty() {
            return None;
        }
        self.guard_target(now);
        let plan = self.pending.pop_front()?;
        // SAFETY: Only the state of the two physical/logical mouse buttons is queried.
        let held = unsafe {
            GetAsyncKeyState(i32::from(VK_LBUTTON)) < 0
                || GetAsyncKeyState(i32::from(VK_RBUTTON)) < 0
        };
        if held || self.button != Some(false) || plan.token > output::TOKEN_MASK {
            log!(
                "stage2_output_blocked token={} reason=button_or_token_guard",
                plan.token
            );
            self.disarm("button_or_token_guard", now);
            return None;
        }
        self.receipts.insert(plan.token, plan.criterion);
        Some(plan)
    }

    pub(super) fn output_result(&mut self, plan: Plan, result: output::Result) {
        let sample = self.sample.map_or("none", |(name, _)| name);
        log!(
            "stage2_output sample={sample} token={} kind={} inserted={} requested=2 win32_error={} repair={:?}",
            plan.token,
            plan.button.label(),
            result.inserted,
            result.error,
            result.repair
        );
        if result.inserted != 2 {
            self.disarm("send_input_incomplete", Instant::now());
        }
    }

    pub(super) fn adjust_movement(&mut self, step: f64) {
        self.parameters.movement = (self.parameters.movement + step).clamp(0.005, 0.2);
        self.parameters.movement_mm = (self.parameters.movement_mm + 100.0 * step).clamp(0.5, 25.0);
        self.show("Movement threshold updated for the next contact".into());
        log!(
            "stage2_parameter movement_mm={:.3} movement_normalized={:.6}",
            self.parameters.movement_mm,
            self.parameters.movement
        );
    }

    pub(super) fn observe(
        &mut self,
        decoded: &Decoded,
        transition: &Option<Transition>,
        scale: Option<Scale>,
        now: Instant,
    ) {
        let Some((name, _)) = self.sample else {
            return;
        };
        if self.button != decoded.physical_button {
            if name == "press_drag"
                && self.mode == Mode::Observe
                && self.button == Some(true)
                && decoded.physical_button == Some(false)
            {
                self.button_verified = true;
            }
            self.button = decoded.physical_button;
            log!(
                "stage2_hid_button sample={name} pressed={:?} at_us={} target_inside={}",
                self.button,
                self.sample_time(now),
                self.target_hit()
            );
        }
        if let Some(Transition::Down { id, start }) = *transition
            && let Some(scale) = scale
        {
            let token = self
                .recognizer
                .begin(id, start, now, scale, self.parameters);
            log!(
                "stage2_down sample={name} token={token} id={id} at_us={} x_span={} y_span={} millimeters={:?} target_inside={}",
                self.sample_time(now),
                scale.x_span,
                scale.y_span,
                scale.millimeters,
                self.target_hit()
            );
        }
        if decoded.physical_button == Some(true) {
            let event = self.recognizer.physical_button(now);
            self.decision(event, now);
        }
        if let Frame::Single(contact) = decoded.frame {
            let event = self.recognizer.position(contact.point, now);
            self.decision(event, now);
        }
        match *transition {
            Some(Transition::Up { end, .. }) => self.finish_contact(Some(end), now, "up"),
            Some(Transition::Incomplete { reason, .. }) => self.finish_contact(None, now, reason),
            _ => {}
        }
        self.sync_timer(now);
    }

    pub(super) fn interrupt(&mut self, reason: &'static str, now: Instant) {
        self.finish_contact(None, now, reason);
        self.sync_timer(now);
    }

    pub(super) fn end(&mut self, reason: &'static str, now: Instant) {
        self.interrupt(reason, now);
        self.pending.clear();
        if !self.receipts.is_empty() {
            log!(
                "stage2_receipts_incomplete tokens={:?}",
                self.receipts.keys().collect::<Vec<_>>()
            );
            self.receipts.clear();
        }
        if let Some((name, _)) = self.sample.take() {
            let counts = &self.counts;
            log!(
                "stage2_end sample={name} reason={reason} left_down={} left_up={} left_double={} right_down={} right_up={} right_double={} wheel={} drags={}",
                counts.left_down,
                counts.left_up,
                counts.left_double,
                counts.right_down,
                counts.right_up,
                counts.right_double,
                counts.wheel,
                counts.drags
            );
            self.show(format!("Finished {name}"));
        }
    }

    pub(super) fn timer(&mut self, token: usize, now: Instant) {
        self.guard_target(now);
        // SAFETY: Read-only query on the UI thread. A lost foreground invalidates the candidate.
        if unsafe { GetForegroundWindow() } != self.window {
            self.interrupt("focus_lost", now);
            return;
        }
        let event = self.recognizer.timer(token, now);
        self.decision(event, now);
        self.sync_timer(now);
    }

    fn sync_timer(&mut self, now: Instant) {
        let deadline = self.recognizer.deadline();
        if deadline.map(|(id, _)| id) == self.timer {
            return;
        }
        if let Some(id) = self.timer.take() {
            // SAFETY: This window owns the timer. Posted stale IDs are harmless.
            unsafe { KillTimer(self.window, id) };
        }
        if let Some((id, due)) = deadline {
            let delay = (due.saturating_duration_since(now).as_millis() + 1) as u32;
            // SAFETY: The window stays live while a stage sample is active.
            if unsafe { SetTimer(self.window, id, delay, None) } != 0 {
                self.timer = Some(id);
            } else {
                log!("stage2_timer_failed {}", crate::api_error("SetTimer"));
                let event = self.recognizer.cancel("timer_error", now);
                self.decision(event, now);
            }
        }
    }

    fn finish_contact(
        &mut self,
        end: Option<crate::contact::Point>,
        now: Instant,
        reason: &'static str,
    ) {
        if let Some((event, summary)) = self.recognizer.finish(end, now, reason) {
            self.decision(event, now);
            let name = self.sample.map_or("none", |(name, _)| name);
            let net = summary
                .net
                .map_or_else(|| "na".into(), |value| format!("{value:.6}"));
            let crossing = summary
                .crossed
                .map_or_else(|| "na".into(), |delay| delay.as_millis().to_string());
            log!(
                "stage2_contact sample={name} token={} id={} duration_ms={} max={:.6} net={net} first_cross_ms={crossing} unit={} limit={:.6} outcome={}",
                summary.token,
                summary.id,
                summary.duration.as_millis(),
                summary.peak,
                summary.unit,
                summary.limit,
                summary.outcome
            );
        }
    }

    fn decision(&mut self, event: Option<Event>, now: Instant) {
        if let Some(event) = event {
            let name = self.sample.map_or("none", |(name, _)| name);
            let label = event.decision.label();
            let reason = event.decision.reason();
            log!(
                "stage2_decision sample={name} token={} id={} at_us={} elapsed_ms={} decision={label} reason={reason} target_inside={}",
                event.token,
                event.id,
                self.sample_time(now),
                event.elapsed.as_millis(),
                self.target_hit()
            );
            let button = match (self.mode, event.decision) {
                (Mode::Both, Decision::Short) => Some(Button::Left),
                (Mode::Both | Mode::RightOnly, Decision::Long) => Some(Button::Right),
                _ => None,
            };
            if let Some(button) = button {
                if self.pending.len() >= 64 || self.receipts.len() >= 64 {
                    self.disarm("output_limit", now);
                } else {
                    let criterion = match button {
                        Button::Left => now,
                        Button::Right => now - event.elapsed + self.parameters.hold,
                    };
                    self.pending.push_back(Plan {
                        token: event.token,
                        button,
                        criterion,
                    });
                }
            }
            self.show(format!(
                "{label} at {} ms ({reason})",
                event.elapsed.as_millis()
            ));
        }
    }

    fn sample_time(&self, now: Instant) -> u128 {
        self.sample
            .map_or(0, |(_, started)| now.duration_since(started).as_micros())
    }

    fn show(&mut self, message: String) {
        self.message = message;
        // SAFETY: Posting avoids reentrant window calls while Probe is borrowed.
        unsafe { PostMessageW(self.window, SHOW_STATUS, 0, 0) };
    }

    pub(super) fn status_text(&self) -> (HWND, Vec<u16>) {
        let counts = &self.counts;
        (
            self.status,
            crate::wide(&format!(
                "{}  |  short <= {} ms  hold >= {} ms  move > {:.1} mm (fallback {:.3})\r\n{}\r\nTarget L {}/{} (dbl {})  R {}/{} (dbl {})  wheel {}  drags {}",
                self.mode.label().to_ascii_uppercase(),
                self.parameters.short.as_millis(),
                self.parameters.hold.as_millis(),
                self.parameters.movement_mm,
                self.parameters.movement,
                self.message,
                counts.left_down + counts.left_double,
                counts.left_up,
                counts.left_double,
                counts.right_down + counts.right_double,
                counts.right_up,
                counts.right_double,
                counts.wheel,
                counts.drags
            )),
        )
    }

    fn target_message(
        &mut self,
        message: u32,
        flags: WPARAM,
        selection: u32,
        now: Instant,
        token: Option<usize>,
    ) {
        let Some((name, _)) = self.sample else {
            return;
        };
        let counts = &mut self.counts;
        let kind = match message {
            WM_LBUTTONDOWN => {
                counts.left_down += 1;
                self.drag_motion = false;
                "left_down"
            }
            WM_LBUTTONUP => {
                counts.left_up += 1;
                if self.drag_motion && selection != 0 {
                    counts.drags += 1;
                }
                self.drag_motion = false;
                "left_up"
            }
            WM_LBUTTONDBLCLK => {
                counts.left_double += 1;
                self.drag_motion = false;
                "left_double"
            }
            WM_RBUTTONDOWN => {
                counts.right_down += 1;
                "right_down"
            }
            WM_RBUTTONUP => {
                counts.right_up += 1;
                "right_up"
            }
            WM_RBUTTONDBLCLK => {
                counts.right_double += 1;
                "right_double"
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                counts.wheel += 1;
                "wheel"
            }
            WM_MOUSEMOVE => {
                self.drag_motion |= flags & 1 != 0;
                return;
            }
            _ => return,
        };
        let anchor = token.and_then(|token| {
            if matches!(message, WM_LBUTTONUP | WM_RBUTTONUP) {
                self.receipts.remove(&token)
            } else {
                self.receipts.get(&token).copied()
            }
        });
        let latency = anchor.map(|anchor| now.saturating_duration_since(anchor).as_micros());
        let source = if token.is_some() { "probe" } else { "untagged" };
        log!(
            "stage2_target sample={name} kind={kind} source={source} at_us={} selection={selection} token={token:?} criterion_to_target_us={latency:?}",
            self.sample_time(now)
        );
        // SAFETY: This is an asynchronous refresh of our own root window.
        unsafe { PostMessageW(self.window, SHOW_STATUS, 0, 0) };
    }
}

pub(super) fn install_target(window: HWND) -> io::Result<()> {
    // SAFETY: The target is the same-thread edit control. Its class default procedure remains in use.
    if unsafe { SetWindowSubclass(window, Some(target_proc), 1, 0) } == 0 {
        return Err(crate::api_error("SetWindowSubclass"));
    }
    Ok(())
}

unsafe extern "system" fn target_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    if message == WM_CONTEXTMENU {
        // Keep the observation target in place after its right-button messages.
        return 0;
    }
    if message == WM_NCDESTROY {
        // SAFETY: Remove only the subclass installed by this module.
        unsafe { RemoveWindowSubclass(window, Some(target_proc), 1) };
    }
    let received = Instant::now();
    // SAFETY: Read the provenance belonging to this message before native edit handling.
    let token = output::tagged_token(unsafe { GetMessageExtraInfo() } as usize);
    // SAFETY: Preserve native edit selection and original window handling.
    let result = unsafe { DefSubclassProc(window, message, wparam, lparam) };
    if message == WM_MOUSELEAVE {
        crate::with_probe(|probe| probe.stage2.disarm("target_left", received));
    }
    if matches!(
        message,
        WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_LBUTTONDBLCLK
            | WM_RBUTTONDOWN
            | WM_RBUTTONUP
            | WM_RBUTTONDBLCLK
            | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL
            | WM_MOUSEMOVE
    ) {
        let mut start = 0u32;
        let mut end = 0u32;
        if message == WM_LBUTTONUP {
            // SAFETY: The pointers are writable u32 values for the synchronous EDIT query.
            unsafe {
                SendMessageW(
                    window,
                    EM_GETSEL,
                    (&mut start as *mut u32) as WPARAM,
                    (&mut end as *mut u32) as LPARAM,
                )
            };
        }
        crate::with_probe(|probe| {
            probe
                .stage2
                .target_message(message, wparam, start.abs_diff(end), received, token)
        });
    }
    result
}
