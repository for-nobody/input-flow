//! Pure rule-matching state machine.
//!
//! The matcher decides, for each observed input event, whether to pass it
//! through or suppress it (hold it while deciding), and — once enough events
//! arrive or a deadline elapses — whether a rule matched (consume + emit an
//! action) or failed (replay the held events in order).
//!
//! It is fully synchronous and deterministic: [`Matcher::on_event`] returns the
//! decision for the current event immediately, and [`Matcher::on_timeout`]
//! advances time explicitly through an injectable [`Clock`], so tests never
//! depend on real sleep. Callers drive time by polling [`Matcher::next_deadline`]
//! and invoking [`Matcher::on_timeout`] once the deadline has elapsed.

use std::cell::Cell;
use std::rc::Rc;

use crate::event::{InputEvent, Key};
use crate::pending::PendingQueue;
use crate::state::KeyState;

/// How to handle the current event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Deliver the event to the target.
    PassThrough,
    /// Withhold the event from the target while deciding.
    Suppress { event_id: u64 },
}

/// Outcome of matching after the current event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Still waiting for more events or a deadline.
    Pending,
    /// A rule matched; consume the trigger and emit the action.
    Matched { rule_id: String, action: Action },
    /// No rule matched; replay the held events in order.
    Failed { replay: Vec<InputEvent> },
}

/// A command produced when a deadline elapses (drives output/replay).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Replay the given events in order.
    Replay { events: Vec<InputEvent> },
    /// Emit a matched rule's action (once).
    Emit { rule_id: String, action: Action },
}

/// Output action carried by a matched rule. Minimal in M3; expanded in M4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    KeyChord(Vec<Key>),
}

/// Logical clock used for timing decisions; injectable for deterministic tests.
pub trait Clock {
    fn now_ms(&self) -> u64;
}

/// A manually advanced clock sharing its current time via interior mutability,
/// so a test can keep one handle while the matcher holds another.
#[derive(Clone)]
pub struct ManualClock(Rc<Cell<u64>>);

impl ManualClock {
    pub fn new(now_ms: u64) -> Self {
        Self(Rc::new(Cell::new(now_ms)))
    }

    pub fn now_ms(&self) -> u64 {
        self.0.get()
    }

    pub fn set(&self, now_ms: u64) {
        self.0.set(now_ms);
    }

    pub fn advance(&self, by_ms: u64) {
        self.0.set(self.0.get() + by_ms);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms()
    }
}

/// A single enabled rule. M3 supports only `Hold`; combo rules arrive in M4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    Hold {
        id: String,
        key: Key,
        timeout_ms: u64,
        action: Action,
    },
}

/// A candidate key being held while its timeout runs.
struct ActiveHold {
    rule_id: String,
    key: Key,
    deadline_ms: u64,
    action: Action,
}

/// The pure matching state machine.
pub struct Matcher {
    clock: Box<dyn Clock>,
    pending: PendingQueue,
    state: KeyState,
    rules: Vec<Rule>,
    active: Option<ActiveHold>,
    paused: bool,
    bypassed: bool,
}

impl Matcher {
    pub fn new(clock: Box<dyn Clock>, rules: Vec<Rule>, pending_capacity: usize) -> Self {
        Self {
            clock,
            pending: PendingQueue::new(pending_capacity),
            state: KeyState::new(),
            rules,
            active: None,
            paused: false,
            bypassed: false,
        }
    }

    /// Feed one event and synchronously decide how to handle it.
    pub fn on_event(&mut self, event: InputEvent) -> (Decision, Resolution) {
        // Synthesized events never participate in matching (FR-06).
        if event.injected || self.paused || self.bypassed {
            return (Decision::PassThrough, Resolution::Pending);
        }

        let Some(key) = event.key() else {
            // Mouse events are not candidates in M3.
            return (Decision::PassThrough, Resolution::Pending);
        };

        // A key whose down was consumed must have its up consumed too (NFR-04),
        // and repeat downs must not re-trigger a match.
        if self.state.is_consumed(key) {
            if event.is_key_up() {
                self.state.clear_consumed(key);
                self.state.mark_physical_up(key);
            }
            return (
                Decision::Suppress {
                    event_id: event.seq,
                },
                Resolution::Pending,
            );
        }

        match self.active.as_ref().map(|a| a.key) {
            Some(active_key) if active_key == key => self.on_active_key_event(key, event),
            Some(_) => self.pass_through(key, event),
            None => self.on_new_event(key, event),
        }
    }

    /// Advance past a deadline and produce any resulting commands.
    ///
    /// Should only be called once the time reported by [`Matcher::next_deadline`]
    /// has been reached or passed; this method no-ops if the deadline is still
    /// in the future.
    pub fn on_timeout(&mut self) -> Vec<Command> {
        let Some(active) = self.active.take() else {
            return Vec::new();
        };
        if self.clock.now_ms() < active.deadline_ms {
            self.active = Some(active);
            return Vec::new();
        }

        if self.state.is_physically_held(active.key) {
            // Held past the threshold: matched. Consume the held down (and any
            // repeats) and emit the action once.
            self.pending.clear();
            self.state.mark_consumed(active.key);
            vec![Command::Emit {
                rule_id: active.rule_id,
                action: active.action,
            }]
        } else {
            // Released before the deadline was processed: replay the held events.
            let replay = self.pending.take_replay();
            self.mark_replay_seen(&replay);
            if replay.is_empty() {
                Vec::new()
            } else {
                vec![Command::Replay { events: replay }]
            }
        }
    }

    /// The next deadline that should trigger [`Matcher::on_timeout`], if any.
    pub fn next_deadline(&self) -> Option<u64> {
        self.active.as_ref().map(|a| a.deadline_ms)
    }

    /// Pause or resume matching. Pausing stops new holds and returns any held
    /// events so the caller can flush them before switching to pass-through.
    pub fn set_paused(&mut self, paused: bool) -> Vec<InputEvent> {
        self.paused = paused;
        if paused {
            let held = self.pending.take_replay();
            self.active = None;
            self.state.clear();
            held
        } else {
            Vec::new()
        }
    }

    /// Whether the matcher has entered bypass (overflow) and should stop
    /// intercepting input.
    pub fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn rule_for_key(&self, key: Key) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|r| matches!(r, Rule::Hold { key: k, .. } if *k == key))
    }

    fn on_new_event(&mut self, key: Key, event: InputEvent) -> (Decision, Resolution) {
        // Only a non-repeat key-down can start a candidate hold.
        if event.is_key_down() && !event.is_repeat() {
            if let Some(rule) = self.rule_for_key(key) {
                let (id, timeout_ms, action) = match rule {
                    Rule::Hold {
                        id,
                        timeout_ms,
                        action,
                        ..
                    } => (id.clone(), *timeout_ms, action.clone()),
                };
                return self.start_hold(key, event, id, timeout_ms, action);
            }
        }
        self.pass_through(key, event)
    }

    fn start_hold(
        &mut self,
        key: Key,
        event: InputEvent,
        id: String,
        timeout_ms: u64,
        action: Action,
    ) -> (Decision, Resolution) {
        if self.pending.push(event).is_err() {
            self.enter_bypass();
            let replay = self.pending.take_replay();
            return (Decision::PassThrough, Resolution::Failed { replay });
        }

        self.state.mark_physical_down(key);
        let deadline = self.clock.now_ms() + timeout_ms;
        self.active = Some(ActiveHold {
            rule_id: id,
            key,
            deadline_ms: deadline,
            action,
        });
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Pending,
        )
    }

    fn on_active_key_event(&mut self, key: Key, event: InputEvent) -> (Decision, Resolution) {
        if event.is_key_up() {
            // Released before the deadline: the hold failed; replay down+up in
            // order (the up is included in the replay, not passed through).
            self.state.mark_physical_up(key);
            if self.pending.push(event).is_err() {
                self.enter_bypass();
                let replay = self.pending.take_replay();
                return (Decision::PassThrough, Resolution::Failed { replay });
            }
            let replay = self.pending.take_replay();
            self.active = None;
            self.mark_replay_seen(&replay);
            return (
                Decision::Suppress {
                    event_id: event.seq,
                },
                Resolution::Failed { replay },
            );
        }

        // A repeat (or a stray extra) down while the hold is active: keep
        // holding; the timer is NOT reset (M5 semantics).
        if self.pending.push(event).is_err() {
            self.enter_bypass();
            let replay = self.pending.take_replay();
            return (Decision::PassThrough, Resolution::Failed { replay });
        }
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Pending,
        )
    }

    fn pass_through(&mut self, key: Key, event: InputEvent) -> (Decision, Resolution) {
        if event.is_key_up() {
            self.state.mark_physical_up(key);
            self.state.mark_seen_up(key);
        } else if event.is_key_down() && !event.is_repeat() {
            self.state.mark_physical_down(key);
            self.state.mark_seen_down(key);
        }
        (Decision::PassThrough, Resolution::Pending)
    }

    fn enter_bypass(&mut self) {
        self.bypassed = true;
        self.active = None;
        self.state.clear();
    }

    fn mark_replay_seen(&mut self, events: &[InputEvent]) {
        for event in events {
            if let Some(key) = event.key() {
                if event.is_key_up() {
                    self.state.mark_seen_up(key);
                } else if !event.is_repeat() {
                    self.state.mark_seen_down(key);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{InputSource, MouseButton, MouseKind};

    fn key_event(seq: u64, time_ms: u64, key: Key, down: bool, repeat: bool) -> InputEvent {
        InputEvent {
            seq,
            time_ms,
            injected: false,
            source: InputSource::Keyboard {
                key,
                scan_code: 0,
                extended: false,
                down,
                repeat,
            },
        }
    }

    fn down(seq: u64, time_ms: u64, key: Key) -> InputEvent {
        key_event(seq, time_ms, key, true, false)
    }

    fn repeat(seq: u64, time_ms: u64, key: Key) -> InputEvent {
        key_event(seq, time_ms, key, true, true)
    }

    fn up(seq: u64, time_ms: u64, key: Key) -> InputEvent {
        key_event(seq, time_ms, key, false, false)
    }

    fn hold_rule(key: Key, timeout_ms: u64) -> Rule {
        Rule::Hold {
            id: "hold".to_string(),
            key,
            timeout_ms,
            action: Action::KeyChord(vec![Key::C]),
        }
    }

    fn matcher_with(clock: ManualClock, rules: Vec<Rule>, capacity: usize) -> Matcher {
        Matcher::new(Box::new(clock), rules, capacity)
    }

    #[test]
    fn hold_down_then_early_up_replays_in_order() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        let d0 = down(0, 0, Key::F8);
        let (decision, resolution) = m.on_event(d0);
        assert_eq!(decision, Decision::Suppress { event_id: 0 });
        assert_eq!(resolution, Resolution::Pending);
        assert_eq!(m.next_deadline(), Some(100));

        clock.advance(30);
        let u0 = up(1, 30, Key::F8);
        let (decision, resolution) = m.on_event(u0);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, u0]
            }
        );
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn repeat_down_does_not_reset_the_timer() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        m.on_event(down(0, 0, Key::F8));
        assert_eq!(m.next_deadline(), Some(100));

        clock.advance(90);
        let r = repeat(1, 90, Key::F8);
        let (decision, resolution) = m.on_event(r);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(resolution, Resolution::Pending);
        // Deadline unchanged by the repeat.
        assert_eq!(m.next_deadline(), Some(100));

        clock.advance(10); // now at 100
        let commands = m.on_timeout();
        assert_eq!(
            commands,
            vec![Command::Emit {
                rule_id: "hold".to_string(),
                action: Action::KeyChord(vec![Key::C]),
            }]
        );
    }

    #[test]
    fn hold_until_timeout_matches_and_consumes_down_and_up() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        m.on_event(down(0, 0, Key::F8));
        clock.advance(120);
        let commands = m.on_timeout();
        assert_eq!(
            commands,
            vec![Command::Emit {
                rule_id: "hold".to_string(),
                action: Action::KeyChord(vec![Key::C]),
            }]
        );
        assert!(!m.is_bypassed());

        // The consumed down's up must be suppressed, not passed through (NFR-04).
        let u0 = up(1, 120, Key::F8);
        let (decision, resolution) = m.on_event(u0);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(resolution, Resolution::Pending);
    }

    #[test]
    fn consumed_key_repeat_downs_do_not_retrigger() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        m.on_event(down(0, 0, Key::F8));
        clock.advance(120);
        m.on_timeout();

        // Repeat downs after a match are suppressed, not treated as a new hold.
        let r = repeat(2, 130, Key::F8);
        let (decision, resolution) = m.on_event(r);
        assert_eq!(decision, Decision::Suppress { event_id: 2 });
        assert_eq!(resolution, Resolution::Pending);
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn non_candidate_key_passes_through() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        let (decision, _) = m.on_event(down(0, 0, Key::A));
        assert_eq!(decision, Decision::PassThrough);
        let (decision, _) = m.on_event(up(1, 1, Key::A));
        assert_eq!(decision, Decision::PassThrough);
    }

    #[test]
    fn injected_events_pass_through() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        let mut ev = down(0, 0, Key::F8);
        ev.injected = true;
        let (decision, resolution) = m.on_event(ev);
        assert_eq!(decision, Decision::PassThrough);
        assert_eq!(resolution, Resolution::Pending);
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn mouse_events_pass_through_in_m3() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        let mouse = InputEvent {
            seq: 0,
            time_ms: 0,
            injected: false,
            source: InputSource::Mouse {
                kind: MouseKind::ButtonDown(MouseButton::Right),
                x: 0,
                y: 0,
            },
        };
        let (decision, _) = m.on_event(mouse);
        assert_eq!(decision, Decision::PassThrough);
    }

    #[test]
    fn overflow_bypasses_and_flushes_held_events() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 1);

        // First candidate down is held.
        let d0 = down(0, 0, Key::F8);
        assert_eq!(m.on_event(d0).0, Decision::Suppress { event_id: 0 });

        // A second candidate down overflows the capacity-1 queue.
        let d1 = down(1, 1, Key::F8);
        let (decision, resolution) = m.on_event(d1);
        assert_eq!(decision, Decision::PassThrough);
        assert_eq!(resolution, Resolution::Failed { replay: vec![d0] });
        assert!(m.is_bypassed());

        // After overflow, everything passes through.
        let (decision, _) = m.on_event(down(2, 2, Key::F8));
        assert_eq!(decision, Decision::PassThrough);
    }

    #[test]
    fn pause_clears_state_and_stops_new_holds() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        let d0 = down(0, 0, Key::F8);
        m.on_event(d0);
        assert_eq!(m.next_deadline(), Some(100));

        let held = m.set_paused(true);
        assert_eq!(held, vec![d0]);
        assert_eq!(m.next_deadline(), None);

        let (decision, _) = m.on_event(down(1, 10, Key::F8));
        assert_eq!(decision, Decision::PassThrough);

        // Resume restores matching.
        m.set_paused(false);
        let (decision, _) = m.on_event(down(2, 20, Key::F8));
        assert_eq!(decision, Decision::Suppress { event_id: 2 });
    }
}
