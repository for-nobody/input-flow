use crate::contact::Point;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) struct Parameters {
    pub short: Duration,
    pub hold: Duration,
    pub movement: f64,
    pub movement_mm: f64,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            short: Duration::from_millis(400),
            hold: Duration::from_millis(500),
            movement: 0.025,
            movement_mm: 3.0,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Scale {
    pub x_span: f64,
    pub y_span: f64,
    pub millimeters: Option<(f64, f64)>,
}

impl Scale {
    fn unit(self) -> &'static str {
        if self.millimeters.is_some() {
            "mm"
        } else {
            "normalized_axis_spans"
        }
    }

    fn movement_limit(self, params: Parameters) -> f64 {
        if self.millimeters.is_some() {
            params.movement_mm
        } else {
            params.movement
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Short,
    Long,
    Suppress(&'static str),
    AfterLong(&'static str),
}

impl Decision {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Long => "long",
            Self::Suppress(_) => "suppress",
            Self::AfterLong(_) => "after_long",
        }
    }

    pub(super) fn reason(self) -> &'static str {
        match self {
            Self::Short => "explicit_up_within_short_limit",
            Self::Long => "hold_deadline_while_down",
            Self::Suppress(reason) | Self::AfterLong(reason) => reason,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Event {
    pub token: usize,
    pub id: i32,
    pub elapsed: Duration,
    pub decision: Decision,
}

#[derive(Clone, Copy)]
enum State {
    Pending,
    Cancelled(&'static str),
    Fired,
}

struct Candidate {
    token: usize,
    id: i32,
    started: Instant,
    start: Point,
    scale: Scale,
    parameters: Parameters,
    state: State,
    peak: f64,
    crossed: Option<Duration>,
    pressed_after_long: bool,
}

impl Candidate {
    fn event(&self, now: Instant, decision: Decision) -> Event {
        Event {
            token: self.token,
            id: self.id,
            elapsed: now.duration_since(self.started),
            decision,
        }
    }

    fn distance(&self, point: Point) -> f64 {
        let (x_units, y_units) = self.scale.millimeters.unwrap_or((1.0, 1.0));
        let x = (f64::from(point.x) - f64::from(self.start.x)) * x_units / self.scale.x_span;
        let y = (f64::from(point.y) - f64::from(self.start.y)) * y_units / self.scale.y_span;
        x.hypot(y)
    }
}

pub(super) struct Summary {
    pub token: usize,
    pub id: i32,
    pub duration: Duration,
    pub peak: f64,
    pub net: Option<f64>,
    pub crossed: Option<Duration>,
    pub outcome: &'static str,
    pub unit: &'static str,
    pub limit: f64,
}

#[derive(Default)]
pub(super) struct Recognizer {
    next_token: usize,
    active: Option<Candidate>,
}

impl Recognizer {
    pub(super) fn begin(
        &mut self,
        id: i32,
        start: Point,
        now: Instant,
        scale: Scale,
        parameters: Parameters,
    ) -> usize {
        self.next_token = self.next_token.wrapping_add(1).max(1);
        self.active = Some(Candidate {
            token: self.next_token,
            id,
            started: now,
            start,
            scale,
            parameters,
            state: State::Pending,
            peak: 0.0,
            crossed: None,
            pressed_after_long: false,
        });
        self.next_token
    }

    pub(super) fn deadline(&self) -> Option<(usize, Instant)> {
        let active = self.active.as_ref()?;
        matches!(active.state, State::Pending)
            .then(|| (active.token, active.started + active.parameters.hold))
    }

    pub(super) fn position(&mut self, point: Point, now: Instant) -> Option<Event> {
        let active = self.active.as_mut()?;
        active.peak = active.peak.max(active.distance(point));
        if active.peak <= active.scale.movement_limit(active.parameters) || active.crossed.is_some()
        {
            return None;
        }
        active.crossed = Some(now.duration_since(active.started));
        match active.state {
            State::Pending => self.cancel("movement", now),
            State::Fired => Some(active.event(now, Decision::AfterLong("movement"))),
            State::Cancelled(_) => None,
        }
    }

    pub(super) fn physical_button(&mut self, now: Instant) -> Option<Event> {
        let active = self.active.as_mut()?;
        if matches!(active.state, State::Fired) && !active.pressed_after_long {
            active.pressed_after_long = true;
            return Some(active.event(now, Decision::AfterLong("physical_button")));
        }
        self.cancel("physical_button", now)
    }

    pub(super) fn cancel(&mut self, reason: &'static str, now: Instant) -> Option<Event> {
        let active = self.active.as_mut()?;
        if !matches!(active.state, State::Pending) {
            return None;
        }
        active.state = State::Cancelled(reason);
        Some(active.event(now, Decision::Suppress(reason)))
    }

    pub(super) fn timer(&mut self, token: usize, now: Instant) -> Option<Event> {
        let active = self.active.as_mut()?;
        if active.token != token
            || !matches!(active.state, State::Pending)
            || now < active.started + active.parameters.hold
        {
            return None;
        }
        active.state = State::Fired;
        Some(active.event(now, Decision::Long))
    }

    pub(super) fn finish(
        &mut self,
        end: Option<Point>,
        now: Instant,
        interruption: &'static str,
    ) -> Option<(Option<Event>, Summary)> {
        let active = self.active.take()?;
        let duration = now.duration_since(active.started);
        let (decision, outcome) = match active.state {
            State::Fired => (None, "long"),
            State::Cancelled(reason) => (None, reason),
            State::Pending if end.is_none() => {
                (Some(Decision::Suppress(interruption)), interruption)
            }
            State::Pending if duration <= active.parameters.short => {
                (Some(Decision::Short), "short")
            }
            State::Pending if duration >= active.parameters.hold => (
                Some(Decision::Suppress("timer_late_after_up")),
                "timer_late_after_up",
            ),
            State::Pending => (Some(Decision::Suppress("timing_gap")), "timing_gap"),
        };
        Some((
            decision.map(|decision| active.event(now, decision)),
            Summary {
                token: active.token,
                id: active.id,
                duration,
                peak: active.peak,
                net: end.map(|point| active.distance(point)),
                crossed: active.crossed,
                outcome,
                unit: active.scale.unit(),
                limit: active.scale.movement_limit(active.parameters),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: Point = Point { x: 100, y: 100 };
    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }
    fn begin(r: &mut Recognizer, now: Instant) -> usize {
        r.begin(
            0,
            ORIGIN,
            now,
            Scale {
                x_span: 4000.0,
                y_span: 2000.0,
                millimeters: None,
            },
            Parameters::default(),
        )
    }
    fn release(r: &mut Recognizer, now: Instant) -> Option<Decision> {
        r.finish(Some(ORIGIN), now, "sample_end")
            .unwrap()
            .0
            .map(|event| event.decision)
    }

    #[test]
    fn short_includes_boundary_and_consecutive_taps_are_independent() {
        let start = Instant::now();
        let mut r = Recognizer::default();
        let old = begin(&mut r, start);
        assert_eq!(release(&mut r, at(start, 400)), Some(Decision::Short));
        let new = begin(&mut r, at(start, 405));
        assert_ne!(old, new);
        assert!(r.timer(old, at(start, 500)).is_none());
        assert_eq!(release(&mut r, at(start, 501)), Some(Decision::Short));
    }

    #[test]
    fn gap_and_up_processed_before_late_timer_never_click() {
        let start = Instant::now();
        let mut r = Recognizer::default();
        for ms in [401, 499, 500, 501] {
            let id = begin(&mut r, start);
            let reason = if ms < 500 {
                "timing_gap"
            } else {
                "timer_late_after_up"
            };
            assert_eq!(
                release(&mut r, at(start, ms)),
                Some(Decision::Suppress(reason))
            );
            assert!(r.timer(id, at(start, 600)).is_none());
        }
    }

    #[test]
    fn timer_fires_once_without_reports_and_up_never_adds_a_short() {
        let start = Instant::now();
        let mut r = Recognizer::default();
        let id = begin(&mut r, start);
        assert!(r.timer(id, at(start, 499)).is_none());
        assert_eq!(
            r.timer(id, at(start, 500)).unwrap().decision,
            Decision::Long
        );
        assert!(r.timer(id, at(start, 501)).is_none());
        assert_eq!(
            r.physical_button(at(start, 525)).unwrap().decision,
            Decision::AfterLong("physical_button")
        );
        assert_eq!(
            r.position(Point { x: 350, y: 100 }, at(start, 540))
                .unwrap()
                .decision,
            Decision::AfterLong("movement")
        );
        assert!(release(&mut r, at(start, 1200)).is_none());
    }

    #[test]
    fn threshold_boundary_and_foldback_keep_the_peak_and_cancellation() {
        let start = Instant::now();
        let mut r = Recognizer::default();
        let id = begin(&mut r, start);
        assert!(
            r.position(Point { x: 200, y: 100 }, at(start, 100))
                .is_none()
        );
        assert_eq!(
            r.position(Point { x: 201, y: 100 }, at(start, 500))
                .unwrap()
                .decision,
            Decision::Suppress("movement")
        );
        r.position(ORIGIN, at(start, 505));
        assert!(r.timer(id, at(start, 600)).is_none());
        let (event, summary) = r.finish(Some(ORIGIN), at(start, 650), "unused").unwrap();
        assert!(event.is_none());
        assert_eq!(summary.net, Some(0.0));
        assert!(summary.peak > 0.025);
        assert_eq!(summary.crossed, Some(Duration::from_millis(500)));
    }

    #[test]
    fn interrupt_at_deadline_beats_timer_and_new_contact_reuses_id_safely() {
        let start = Instant::now();
        let mut r = Recognizer::default();
        for reason in ["focus_lost", "multi", "device_removed", "sample_end"] {
            let old = begin(&mut r, start);
            assert_eq!(
                r.finish(None, at(start, 500), reason)
                    .unwrap()
                    .0
                    .unwrap()
                    .decision,
                Decision::Suppress(reason)
            );
            let new = begin(&mut r, at(start, 501));
            assert!(r.timer(old, at(start, 1001)).is_none());
            assert_eq!(
                r.timer(new, at(start, 1001)).unwrap().decision,
                Decision::Long
            );
            r.finish(None, at(start, 1002), reason);
        }
    }
}
