use std::time::{Duration, Instant};

use inputflow_config::{MovementMode, Preset, Style, parse_color};
use inputflow_touchpad::contact::{Frame, Point};

pub(super) const MAX_SEGMENTS: usize = 16_384;
type Pixel = (i32, i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
    pub fn intersect(self, other: Self) -> Option<Self> {
        let overlap = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        (overlap.left < overlap.right && overlap.top < overlap.bottom).then_some(overlap)
    }
    pub fn around((x, y): Pixel, radius: i32) -> Self {
        Self {
            left: x - radius,
            top: y - radius,
            right: x + radius + 1,
            bottom: y + radius + 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Brush {
    Ring {
        color: u32,
        radius: i32,
        width: i32,
    },
    Dot {
        color: u32,
        radius: i32,
    },
    Ink {
        color: u32,
        width: i32,
        retention: Option<Duration>,
    },
}

impl Brush {
    pub fn from_preset(preset: &Preset) -> Option<Self> {
        Some(match &preset.style {
            Style::Ring {
                color,
                diameter,
                thickness,
            } => Self::Ring {
                color: parse_color(color)?,
                radius: (*diameter / 2) as i32,
                width: *thickness as i32,
            },
            Style::Dot { color, diameter } => Self::Dot {
                color: parse_color(color)?,
                radius: (*diameter / 2) as i32,
            },
            Style::Ink {
                color,
                width,
                retain_seconds,
            } => Self::Ink {
                color: parse_color(color)?,
                width: *width as i32,
                retention: retain_seconds.map(|seconds| Duration::from_secs(u64::from(seconds))),
            },
        })
    }
    pub fn pointer(self) -> (u32, i32, i32) {
        match self {
            Self::Ring {
                color,
                radius,
                width,
            } => (color, radius, radius - width),
            Self::Dot { color, radius } => (color, radius, 0),
            Self::Ink { color, width, .. } => (color, (width / 2).max(2), 0),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Segment {
    pub from: Pixel,
    pub to: Pixel,
    pub color: u32,
    pub radius: i32,
    pub expires: Option<Instant>,
}

impl Segment {
    pub fn bounds(self) -> Rect {
        Rect::around(self.from, self.radius).union(Rect::around(self.to, self.radius))
    }
}

struct Contact {
    device: usize,
    id: i32,
    last: Point,
}

pub(super) struct Session {
    pub brush: Brush,
    pub preset_id: String,
    pub mouse_position: Pixel,
    pub temporary: bool,
    pub owner: Option<usize>,
    pub ink: Vec<Segment>,
    pub capacity_reached: bool,
    position: (f64, f64),
    bounds: Rect,
    mode: MovementMode,
    contact: Option<Contact>,
    waiting_for_clear: bool,
    dirty: Option<Rect>,
    next_expiry: Option<Instant>,
}

impl Session {
    pub fn new(
        width: i32,
        height: i32,
        mode: MovementMode,
        preset: &Preset,
        mouse_position: Pixel,
        occupied: bool,
    ) -> Option<Self> {
        let bounds = Rect {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        Some(Self {
            brush: Brush::from_preset(preset)?,
            preset_id: preset.id.clone(),
            mouse_position,
            temporary: false,
            owner: None,
            ink: Vec::new(),
            capacity_reached: false,
            position: (f64::from(width / 2), f64::from(height / 2)),
            bounds,
            mode,
            contact: None,
            waiting_for_clear: occupied,
            dirty: Some(bounds),
            next_expiry: None,
        })
    }

    pub fn pointer(&self) -> Pixel {
        (
            self.position.0.round() as i32,
            self.position.1.round() as i32,
        )
    }

    pub fn take_dirty(&mut self) -> Option<Rect> {
        self.dirty
            .take()
            .and_then(|dirty| dirty.intersect(self.bounds))
    }

    fn dirty(&mut self, rect: Rect) {
        self.dirty = Some(self.dirty.map_or(rect, |previous| rect.union(previous)));
    }

    fn mark_pointer(&mut self) {
        self.dirty(Rect::around(self.pointer(), self.brush.pointer().1));
    }

    pub fn interrupt(&mut self, occupied: bool) {
        self.contact = None;
        self.waiting_for_clear = occupied;
    }

    pub fn select(&mut self, preset: &Preset, mouse: Pixel, occupied: bool) {
        let Some(brush) = Brush::from_preset(preset) else {
            return;
        };
        self.mark_pointer();
        if self.temporary {
            self.mouse_position = mouse;
        }
        self.temporary = false;
        self.brush = brush;
        self.preset_id.clone_from(&preset.id);
        self.interrupt(occupied);
        self.mark_pointer();
    }

    pub fn toggle_mouse(&mut self, actual_mouse: Pixel, occupied: bool) -> Option<Pixel> {
        self.mark_pointer();
        self.temporary = !self.temporary;
        self.interrupt(occupied);
        if self.temporary {
            Some(self.mouse_position)
        } else {
            self.mouse_position = actual_mouse;
            None
        }
    }

    pub fn clear(&mut self, occupied: bool) {
        self.ink.clear();
        self.capacity_reached = false;
        self.next_expiry = None;
        self.interrupt(occupied);
        self.dirty = Some(self.bounds);
    }

    pub fn expire(&mut self, now: Instant) {
        if self.next_expiry.is_none_or(|next| now < next) {
            return;
        }
        self.next_expiry = None;
        let mut dirty = self.dirty;
        self.ink.retain(|segment| {
            if let Some(expires) = segment.expires {
                if now >= expires {
                    let bounds = segment.bounds();
                    dirty = Some(dirty.map_or(bounds, |old| old.union(bounds)));
                    return false;
                }
                self.next_expiry = Some(self.next_expiry.map_or(expires, |old| old.min(expires)));
            }
            true
        });
        self.dirty = dirty;
    }

    pub fn frame(
        &mut self,
        device: usize,
        frame: Frame,
        logical: [(i32, i32); 2],
        competing: bool,
        now: Instant,
    ) {
        if self.temporary {
            return;
        }
        if competing || matches!(frame, Frame::Multiple) {
            self.interrupt(true);
            return;
        }
        let Frame::Single(sample) = frame else {
            self.interrupt(false);
            return;
        };
        if !sample.tip {
            if self.contact.as_ref().is_none_or(|old| old.device == device) {
                self.interrupt(false);
            }
            return;
        }
        if self.waiting_for_clear {
            return;
        }
        let previous = self.contact.take();
        if previous
            .as_ref()
            .is_some_and(|old| old.device != device || old.id != sample.id)
        {
            self.interrupt(true);
            return;
        }
        self.owner = Some(device);
        self.contact = Some(Contact {
            device,
            id: sample.id,
            last: sample.point,
        });
        let [x_axis, y_axis] = logical;
        let span = |(min, max)| f64::from(max) - f64::from(min);
        if span(x_axis) <= 0.0 || span(y_axis) <= 0.0 {
            return;
        }
        let old = self.pointer();
        let width = f64::from(self.bounds.right - 1);
        let height = f64::from(self.bounds.bottom - 1);
        let axis = |value, (min, max)| (f64::from(value) - f64::from(min)) / span((min, max));
        let next = match (&self.mode, &previous) {
            (MovementMode::Absolute, _) => (
                axis(sample.point.x, x_axis) * width,
                axis(sample.point.y, y_axis) * height,
            ),
            (MovementMode::Relative, Some(previous)) => {
                let gain = width / span(x_axis);
                (
                    self.position.0
                        + (f64::from(sample.point.x) - f64::from(previous.last.x)) * gain,
                    self.position.1
                        + (f64::from(sample.point.y) - f64::from(previous.last.y)) * gain,
                )
            }
            (MovementMode::Relative, None) => return,
        };
        let next = (next.0.clamp(0.0, width), next.1.clamp(0.0, height));
        let radius = self.brush.pointer().1;
        self.position = next;
        let new = self.pointer();
        if old == new {
            return;
        }
        self.dirty(Rect::around(old, radius).union(Rect::around(new, radius)));
        if previous.is_some()
            && let Brush::Ink {
                color,
                width,
                retention,
            } = self.brush
        {
            let segment = Segment {
                from: old,
                to: new,
                color,
                radius: (width / 2).max(1),
                expires: retention.map(|duration| now + duration),
            };
            if self.ink.len() == MAX_SEGMENTS {
                self.capacity_reached = true;
                return;
            }
            if let Some(expires) = segment.expires {
                self.next_expiry = Some(self.next_expiry.map_or(expires, |old| old.min(expires)));
            }
            self.dirty(segment.bounds());
            self.ink.push(segment);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inputflow_touchpad::contact::Contact as ReportContact;

    const AXES: [(i32, i32); 2] = [(-10, 990), (20, 520)];

    fn ink(retain_seconds: Option<u32>) -> Preset {
        Preset {
            id: "ink".into(),
            name: "Ink".into(),
            binding: None,
            style: Style::Ink {
                color: "#3366FF".into(),
                width: 5,
                retain_seconds,
            },
        }
    }

    fn contact(tip: bool, x: i32, y: i32) -> Frame {
        Frame::Single(ReportContact {
            id: 7,
            tip,
            point: Point { x, y },
        })
    }

    #[test]
    fn relative_recontact_and_mouse_roundtrip_do_not_join_positions() {
        let now = Instant::now();
        let mut session = Session::new(
            1001,
            501,
            MovementMode::Relative,
            &ink(None),
            (-40, 600),
            false,
        )
        .unwrap();
        session.frame(1, contact(true, 100, 100), AXES, false, now);
        assert_eq!(session.pointer(), (500, 250));
        session.frame(1, contact(true, 160, 130), AXES, false, now);
        let b = session.pointer();
        assert_eq!(b, (560, 280));
        assert_eq!(session.ink.len(), 1);
        assert_eq!(session.toggle_mouse((1, 2), true), Some((-40, 600)));
        assert_eq!(session.ink.len(), 1);
        assert!(session.toggle_mouse((80, 120), true).is_none());
        session.frame(1, contact(true, 300, 300), AXES, false, now);
        assert_eq!(session.pointer(), b);
        assert_eq!(session.mouse_position, (80, 120));
        session.frame(1, contact(false, 300, 300), AXES, false, now);
        session.frame(1, contact(true, 900, 400), AXES, false, now);
        assert_eq!(session.pointer(), b);
        assert_eq!(session.ink.len(), 1);
        assert_eq!(session.toggle_mouse((0, 0), true), Some((80, 120)));
    }

    #[test]
    fn absolute_uses_inclusive_endpoints_and_never_draws_on_a_new_down() {
        let now = Instant::now();
        let mut session = Session::new(
            1921,
            1081,
            MovementMode::Absolute,
            &ink(None),
            (-130, 0),
            false,
        )
        .unwrap();
        for (x, y, expected) in [
            (-10, 20, (0, 0)),
            (490, 270, (960, 540)),
            (990, 520, (1920, 1080)),
        ] {
            session.frame(1, contact(true, x, y), AXES, false, now);
            assert_eq!(session.pointer(), expected);
            assert!(session.ink.is_empty());
            session.frame(1, contact(false, x, y), AXES, false, now);
        }
        session.frame(1, contact(true, 500, 200), AXES, false, now);
        session.frame(1, Frame::Multiple, AXES, false, now);
        let before = session.pointer();
        session.frame(1, contact(true, -10, 20), AXES, false, now);
        assert_eq!(session.pointer(), before);
        assert!(session.ink.is_empty());
    }

    #[test]
    fn timed_segments_expire_independently_and_clear_breaks_the_old_stroke() {
        let now = Instant::now();
        let mut session = Session::new(
            1001,
            501,
            MovementMode::Relative,
            &ink(Some(3)),
            (0, 0),
            false,
        )
        .unwrap();
        session.frame(1, contact(true, 100, 100), AXES, false, now);
        session.frame(1, contact(true, 110, 100), AXES, false, now);
        session.frame(
            1,
            contact(true, 120, 100),
            AXES,
            false,
            now + Duration::from_secs(2),
        );
        assert_eq!(session.ink.len(), 2);
        session.toggle_mouse((0, 0), true);
        session.expire(now + Duration::from_secs(3));
        assert_eq!(session.ink.len(), 1);
        session.expire(now + Duration::from_secs(5));
        assert!(session.ink.is_empty());
        session.select(&ink(None), (0, 0), false);
        session.frame(1, contact(true, 120, 100), AXES, false, now);
        session.frame(1, contact(true, 130, 100), AXES, false, now);
        session.clear(true);
        session.frame(1, contact(true, 140, 100), AXES, false, now);
        assert!(session.ink.is_empty());
        session.frame(1, contact(false, 140, 100), AXES, false, now);
        session.frame(1, contact(true, 700, 100), AXES, false, now);
        session.frame(1, contact(true, 720, 100), AXES, false, now);
        assert_eq!(session.ink.len(), 1);
    }
}
