use std::time::Instant;

use crate::contact::{Frame, Point, Tracker, Transition};

pub const DIGITS: [[Option<u8>; 3]; 4] = [
    [Some(7), Some(8), Some(9)],
    [Some(4), Some(5), Some(6)],
    [Some(1), Some(2), Some(3)],
    [None, Some(0), None],
];
pub const CELL: i32 = 100;
const HALF_GUTTER: i32 = 4;
pub const WIDTH: i32 = 3 * CELL;
pub const HEIGHT: i32 = 4 * CELL;

/// Inclusive outer edges; internal edges of these rectangles belong to the gutter.
pub fn cell_bounds(row: usize, column: usize) -> [i32; 4] {
    let (left, right) = axis_bounds(column as i32, WIDTH);
    let (top, bottom) = axis_bounds(row as i32, HEIGHT);
    [left, top, right, bottom]
}

fn axis_bounds(index: i32, extent: i32) -> (i32, i32) {
    let start = index * CELL;
    let end = start + CELL;
    (
        if start == 0 { 0 } else { start + HALF_GUTTER },
        if end == extent {
            extent
        } else {
            end - HALF_GUTTER
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Digit(u8),
    Gutter,
    Blank,
    Outside,
}

#[derive(Clone, Copy)]
pub struct Layout([(i32, i32); 2]);

impl Layout {
    pub fn new(bounds: [(i32, i32); 2]) -> Option<Self> {
        bounds
            .iter()
            .all(|(min, max)| min < max)
            .then_some(Self(bounds))
    }

    fn hit(self, point: Point) -> Hit {
        let axis = |coordinate, (min, max), extent| {
            if !(min..=max).contains(&coordinate) {
                return None;
            }
            let span = i64::from(max) - i64::from(min);
            let units = (i64::from(coordinate) - i64::from(min)) * i64::from(extent);
            let index = (units / (span * i64::from(CELL))).min(i64::from(extent / CELL - 1));
            let (low, high) = axis_bounds(index as i32, extent);
            let gutter = (low > 0 && units <= i64::from(low) * span)
                || (high < extent && units >= i64::from(high) * span);
            Some((index as usize, gutter))
        };
        match (
            axis(point.x, self.0[0], WIDTH),
            axis(point.y, self.0[1], HEIGHT),
        ) {
            (Some((_, true)), Some(_)) | (Some(_), Some((_, true))) => Hit::Gutter,
            (Some((column, false)), Some((row, false))) => {
                DIGITS[row][column].map_or(Hit::Blank, Hit::Digit)
            }
            _ => Hit::Outside,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Highlight(Option<u8>),
    Confirmed(u8),
    Cancelled(&'static str),
}

#[derive(Default)]
pub struct Recognizer {
    tracker: Tracker,
    selected: Option<u8>,
    highlight: Option<u8>,
}

impl Recognizer {
    pub fn highlight(&self) -> Option<u8> {
        self.highlight
    }

    pub fn interrupt(&mut self, reason: &'static str) -> Option<Event> {
        self.tracker.interrupt(reason);
        self.clear(reason)
    }

    fn clear(&mut self, reason: &'static str) -> Option<Event> {
        self.highlight = None;
        self.selected.take().map(|_| Event::Cancelled(reason))
    }

    pub fn observe(&mut self, frame: Frame, layout: Layout, now: Instant) -> Option<Event> {
        match self.tracker.observe(frame, now) {
            Some(Transition::Down { start, .. }) => {
                let Hit::Digit(digit) = layout.hit(start) else {
                    return Some(Event::Cancelled("down_outside_digit"));
                };
                self.selected = Some(digit);
                self.highlight = Some(digit);
                Some(Event::Highlight(self.highlight))
            }
            Some(Transition::Up { end, .. }) => {
                let digit = self.selected.take()?;
                self.highlight = None;
                Some(if layout.hit(end) == Hit::Digit(digit) {
                    Event::Confirmed(digit)
                } else {
                    Event::Cancelled("up_outside_origin")
                })
            }
            Some(Transition::Incomplete { reason, .. }) => self.clear(reason),
            None => {
                let digit = self.selected?;
                let Frame::Single(contact) = frame else {
                    return None;
                };
                match layout.hit(contact.point) {
                    Hit::Digit(hit) if hit == digit => self.set_highlight(Some(digit)),
                    Hit::Gutter => self.set_highlight(None),
                    Hit::Digit(_) => self.clear("crossed_digit"),
                    Hit::Blank => self.clear("entered_blank"),
                    Hit::Outside => self.clear("invalid_coordinate"),
                }
            }
        }
    }

    fn set_highlight(&mut self, next: Option<u8>) -> Option<Event> {
        if self.highlight == next {
            return None;
        }
        self.highlight = next;
        Some(Event::Highlight(next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::Contact;
    use std::time::Duration;

    const LAYOUT: Layout = Layout([(0, WIDTH), (0, HEIGHT)]);
    const FIVE: Point = Point { x: 150, y: 150 };
    const SIX: Point = Point { x: 250, y: 150 };
    const GAP: Point = Point { x: 200, y: 150 };
    const BLANK: Point = Point { x: 50, y: 350 };

    fn sample(point: Point, tip: bool) -> Frame {
        Frame::Single(Contact { id: 0, tip, point })
    }

    #[test]
    fn geometry_agrees_with_guide_and_includes_outer_endpoints() {
        for (row, digits) in DIGITS.iter().enumerate() {
            for (column, digit) in digits.iter().enumerate() {
                let [left, top, right, bottom] = cell_bounds(row, column);
                assert_eq!(
                    LAYOUT.hit(Point {
                        x: (left + right) / 2,
                        y: (top + bottom) / 2
                    }),
                    digit.map_or(Hit::Blank, Hit::Digit)
                );
            }
        }
        for x in [196, 200, 204] {
            assert_eq!(LAYOUT.hit(Point { x, y: 150 }), Hit::Gutter);
        }
        for y in [196, 200, 204] {
            assert_eq!(LAYOUT.hit(Point { x: 150, y }), Hit::Gutter);
        }
        assert_eq!(LAYOUT.hit(Point { x: 195, y: 150 }), Hit::Digit(5));
        assert_eq!(LAYOUT.hit(Point { x: 205, y: 150 }), Hit::Digit(6));
        assert_eq!(LAYOUT.hit(Point { x: WIDTH, y: 0 }), Hit::Digit(9));
        assert_eq!(LAYOUT.hit(Point { x: 0, y: 0 }), Hit::Digit(7));
        assert_eq!(LAYOUT.hit(Point { x: 150, y: HEIGHT }), Hit::Digit(0));
        assert_eq!(
            LAYOUT.hit(Point {
                x: WIDTH,
                y: HEIGHT
            }),
            Hit::Blank
        );
        assert_eq!(LAYOUT.hit(Point { x: -1, y: 0 }), Hit::Outside);
        let offset = Layout::new([(-30, 570), (20, 1_220)]).unwrap();
        assert_eq!(offset.hit(Point { x: 270, y: 470 }), Hit::Digit(5));
        assert!(Layout::new([(1, 1), (0, 1)]).is_none());
    }

    #[test]
    fn returning_from_a_gutter_can_confirm_but_crossing_a_digit_or_blank_cannot() {
        let mut pad = Recognizer::default();
        let now = Instant::now();
        let mut report = |point, tip| pad.observe(sample(point, tip), LAYOUT, now);
        assert_eq!(report(FIVE, true), Some(Event::Highlight(Some(5))));
        assert_eq!(report(GAP, true), Some(Event::Highlight(None)));
        assert_eq!(report(FIVE, true), Some(Event::Highlight(Some(5))));
        assert_eq!(report(FIVE, false), Some(Event::Confirmed(5)));
        for (point, reason) in [(SIX, "crossed_digit"), (BLANK, "entered_blank")] {
            assert_eq!(report(FIVE, true), Some(Event::Highlight(Some(5))));
            assert_eq!(report(point, true), Some(Event::Cancelled(reason)));
            assert_eq!(report(FIVE, true), None);
            assert_eq!(report(FIVE, false), None);
        }
        report(FIVE, true);
        assert_eq!(
            report(GAP, false),
            Some(Event::Cancelled("up_outside_origin"))
        );
        for point in [GAP, BLANK] {
            assert_eq!(
                report(point, true),
                Some(Event::Cancelled("down_outside_digit"))
            );
            assert_eq!(report(FIVE, true), None);
            assert_eq!(report(FIVE, false), None);
        }
    }

    #[test]
    fn only_the_matching_explicit_lift_confirms_and_contact_id_can_be_reused() {
        let mut pad = Recognizer::default();
        let now = Instant::now();
        for _ in 0..2 {
            pad.observe(sample(FIVE, true), LAYOUT, now);
            let later = now + Duration::from_secs(30);
            assert_eq!(pad.observe(sample(FIVE, true), LAYOUT, later), None);
            assert_eq!(
                pad.observe(sample(FIVE, false), LAYOUT, later),
                Some(Event::Confirmed(5))
            );
            assert_eq!(pad.observe(sample(FIVE, false), LAYOUT, later), None);
        }
        pad.observe(sample(FIVE, true), LAYOUT, now);
        assert_eq!(
            pad.observe(Frame::Empty, LAYOUT, now),
            Some(Event::Cancelled("empty_without_tip_clear"))
        );
        assert_eq!(pad.observe(sample(FIVE, false), LAYOUT, now), None);
        pad.observe(sample(FIVE, true), LAYOUT, now);
        assert_eq!(
            pad.observe(Frame::Multiple, LAYOUT, now),
            Some(Event::Cancelled("multiple_contacts"))
        );
        assert_eq!(pad.observe(sample(FIVE, true), LAYOUT, now), None);
        assert_eq!(pad.observe(sample(FIVE, false), LAYOUT, now), None);
        pad.observe(sample(FIVE, true), LAYOUT, now);
        assert_eq!(
            pad.interrupt("physical_button"),
            Some(Event::Cancelled("physical_button"))
        );
        assert_eq!(pad.observe(sample(FIVE, true), LAYOUT, now), None);
        assert_eq!(pad.observe(sample(FIVE, false), LAYOUT, now), None);
    }
}
