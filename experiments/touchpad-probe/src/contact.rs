use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy)]
pub(super) struct Contact {
    pub id: i32,
    pub tip: bool,
    pub point: Point,
}

#[derive(Clone, Copy)]
pub(super) enum Frame {
    Empty,
    Single(Contact),
    Multiple,
}

struct Active {
    id: i32,
    start: Point,
    started: Instant,
    last_report: Instant,
    max_gap: Duration,
    reports: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Transition {
    Down {
        id: i32,
        start: Point,
    },
    Up {
        id: i32,
        start: Point,
        end: Point,
        duration: Duration,
        max_gap: Duration,
        reports: u64,
    },
    Incomplete {
        id: i32,
        reason: &'static str,
    },
}

#[derive(Default)]
pub(super) struct Tracker {
    active: Option<Active>,
    waiting_for_clear: bool,
}

impl Tracker {
    pub(super) fn observe(&mut self, frame: Frame, now: Instant) -> Option<Transition> {
        match frame {
            Frame::Multiple => self.interrupt("multiple_contacts"),
            Frame::Empty => {
                self.waiting_for_clear = false;
                self.abandon("empty_without_tip_clear")
            }
            Frame::Single(contact) if !contact.tip => {
                self.waiting_for_clear = false;
                let active = self.active.take()?;
                if active.id != contact.id {
                    return Some(Transition::Incomplete {
                        id: active.id,
                        reason: "different_id_on_lift",
                    });
                }
                Some(Transition::Up {
                    id: active.id,
                    start: active.start,
                    end: contact.point,
                    duration: now.duration_since(active.started),
                    max_gap: active.max_gap.max(now.duration_since(active.last_report)),
                    reports: active.reports + 1,
                })
            }
            Frame::Single(_) if self.waiting_for_clear => None,
            Frame::Single(contact) => {
                if let Some(active) = &mut self.active {
                    if active.id != contact.id {
                        return self.interrupt("id_changed_without_tip_clear");
                    }
                    active.max_gap = active.max_gap.max(now.duration_since(active.last_report));
                    active.last_report = now;
                    active.reports += 1;
                    return None;
                }
                self.active = Some(Active {
                    id: contact.id,
                    start: contact.point,
                    started: now,
                    last_report: now,
                    max_gap: Duration::ZERO,
                    reports: 1,
                });
                Some(Transition::Down {
                    id: contact.id,
                    start: contact.point,
                })
            }
        }
    }

    pub(super) fn interrupt(&mut self, reason: &'static str) -> Option<Transition> {
        self.waiting_for_clear = true;
        self.abandon(reason)
    }

    fn abandon(&mut self, reason: &'static str) -> Option<Transition> {
        self.active.take().map(|active| Transition::Incomplete {
            id: active.id,
            reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finger(tip: bool, x: i32) -> Frame {
        Frame::Single(Contact {
            id: 0,
            tip,
            point: Point { x, y: 100 },
        })
    }

    #[test]
    fn silence_preserves_contact_until_matching_explicit_tip_clear() {
        let mut tracker = Tracker::default();
        let start = Instant::now();
        assert!(matches!(
            tracker.observe(finger(true, 50), start),
            Some(Transition::Down { id: 0, .. })
        ));
        assert_eq!(
            tracker.observe(finger(true, 60), start + Duration::from_secs(1)),
            None
        );
        let elapsed = Duration::from_millis(1020);
        assert_eq!(
            tracker.observe(finger(false, 60), start + elapsed),
            Some(Transition::Up {
                id: 0,
                start: Point { x: 50, y: 100 },
                end: Point { x: 60, y: 100 },
                duration: elapsed,
                max_gap: Duration::from_secs(1),
                reports: 3,
            })
        );
        assert!(matches!(
            tracker.observe(finger(true, 80), start + elapsed),
            Some(Transition::Down { id: 0, .. })
        ));
    }

    #[test]
    fn multi_contact_frames_cannot_turn_into_a_single_contact_lifecycle() {
        let mut tracker = Tracker::default();
        let now = Instant::now();
        tracker.observe(finger(true, 50), now);
        assert!(matches!(
            tracker.observe(Frame::Multiple, now),
            Some(Transition::Incomplete { .. })
        ));
        assert_eq!(tracker.observe(finger(true, 80), now), None);
        assert_eq!(tracker.observe(finger(false, 80), now), None);
        assert!(matches!(
            tracker.observe(finger(true, 90), now),
            Some(Transition::Down { .. })
        ));
    }

    #[test]
    fn absent_or_different_contact_never_counts_as_an_up() {
        let mut tracker = Tracker::default();
        let now = Instant::now();
        tracker.observe(finger(true, 50), now);
        assert!(matches!(
            tracker.observe(Frame::Empty, now),
            Some(Transition::Incomplete { .. })
        ));
        tracker.observe(finger(true, 60), now);
        let other = Frame::Single(Contact {
            id: 1,
            tip: false,
            point: Point { x: 60, y: 100 },
        });
        assert!(matches!(
            tracker.observe(other, now),
            Some(Transition::Incomplete { .. })
        ));
    }
}
