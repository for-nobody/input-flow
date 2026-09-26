//! Pure rule-matching state machine.
//!
//! Rules are precompiled into a [`RuleIndex`]; the matcher performs only cheap
//! lookups per event. `Key+Key` and `Key+MouseButton` chords hold the first key
//! down and wait for a matching second input; a non-matching second input, or
//! the first key being released, fails the chord and replays the held events in
//! order. Single-key `Hold` rules keep a deadline resolved via
//! [`Matcher::on_timeout`].
//!
//! The matcher is synchronous and deterministic: [`Matcher::on_event`] returns
//! the decision for the current event immediately, and [`Matcher::on_timeout`]
//! advances time explicitly through an injectable [`Clock`].

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::event::{InputEvent, InputSource, Key, MouseButton, MouseKind};
use crate::pending::PendingQueue;
use crate::rules::{Action, RuleIndex};
use crate::state::{KeyState, MouseState};

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

/// Logical clock used for timing decisions; injectable for deterministic tests.
pub trait Clock {
    fn now_ms(&self) -> u64;
}

/// A manually advanced clock sharing its current time via interior mutability,
/// so a test can keep one handle while the matcher holds another.
#[derive(Clone)]
pub struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    pub fn new(now_ms: u64) -> Self {
        Self(Arc::new(AtomicU64::new(now_ms)))
    }

    pub fn now_ms(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, now_ms: u64) {
        self.0.store(now_ms, Ordering::Relaxed);
    }

    pub fn advance(&self, by_ms: u64) {
        self.0.fetch_add(by_ms, Ordering::Relaxed);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms()
    }
}

/// A clock backed by the system monotonic clock. `Send` so a [`Matcher`] can be
/// shared with a hook thread.
#[derive(Debug, Clone)]
pub struct SystemClock {
    start: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl SystemClock {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

/// A single-key `Hold` rule currently running its timeout.
struct ActiveHold {
    rule_id: String,
    key: Key,
    deadline_ms: u64,
    action: Action,
}

/// A `Key+Key` / `Key+MouseButton` chord awaiting its second input.
struct ActiveChord {
    first: Key,
}

/// A `Hold+MouseButton` rule: first wait for the key's threshold, then wait for
/// the button to be pressed while the key is still held.
#[derive(Clone)]
struct ActiveHoldButton {
    rule_id: String,
    key: Key,
    button: MouseButton,
    deadline_ms: u64,
    /// `true` once the threshold has elapsed and the key is still held; the
    /// matcher is now waiting for the button down.
    armed: bool,
    action: Action,
}

/// The active prefix being matched, if any.
enum Active {
    Holding(ActiveHold),
    Chording(ActiveChord),
    HoldToButton(ActiveHoldButton),
}

/// The pure matching state machine.
pub struct Matcher {
    clock: Box<dyn Clock + Send>,
    pending: PendingQueue,
    keys: KeyState,
    buttons: MouseState,
    index: RuleIndex,
    active: Option<Active>,
    paused: bool,
    bypassed: bool,
}

impl Matcher {
    /// Create a matcher over a precompiled rule index.
    pub fn new(clock: Box<dyn Clock + Send>, index: RuleIndex, pending_capacity: usize) -> Self {
        Self {
            clock,
            pending: PendingQueue::new(pending_capacity),
            keys: KeyState::new(),
            buttons: MouseState::new(),
            index,
            active: None,
            paused: false,
            bypassed: false,
        }
    }

    /// Feed one event and synchronously decide how to handle it.
    pub fn on_event(&mut self, event: InputEvent) -> (Decision, Resolution) {
        // Synthesized events never participate in matching (FR-06).
        if event.injected || self.paused || self.bypassed {
            return self.pass_through(event);
        }

        // A key/button whose down was consumed must have its up consumed too
        // (NFR-04), and repeat downs must not re-trigger a match.
        if let Some(key) = event.key()
            && self.keys.is_consumed(key)
        {
            return self.consume_key(key, event);
        }
        if let Some(button) = event.button()
            && self.buttons.is_consumed(button)
        {
            return self.consume_button(button, event);
        }

        // No enabled rules: nothing is a candidate, everything passes through.
        if self.index.is_empty() {
            return self.pass_through(event);
        }

        match event.source {
            InputSource::Keyboard { .. } => self.on_key_event(event),
            InputSource::Mouse {
                kind: MouseKind::ButtonDown(_) | MouseKind::ButtonUp(_),
                ..
            } => self.on_button_event(event),
            InputSource::Mouse { .. } => self.pass_through(event),
        }
    }

    /// Advance past a deadline and produce any resulting commands. Single-key
    /// `Hold` rules emit their action (or replay if released); `Hold+MouseButton`
    /// rules arm themselves at the deadline and then wait for the button down.
    pub fn on_timeout(&mut self) -> Vec<Command> {
        match self.active.take() {
            Some(Active::Holding(active)) => {
                if self.clock.now_ms() < active.deadline_ms {
                    self.active = Some(Active::Holding(active));
                    return Vec::new();
                }

                if self.keys.is_physically_held(active.key) {
                    // Held past the threshold: matched. Consume the held down (and
                    // any repeats) and emit the action once.
                    self.pending.clear();
                    self.keys.mark_consumed(active.key);
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
            Some(Active::HoldToButton(mut active)) => {
                if active.armed {
                    // Already armed and waiting for the button; no deadline to fire.
                    self.active = Some(Active::HoldToButton(active));
                    return Vec::new();
                }
                if self.clock.now_ms() < active.deadline_ms {
                    self.active = Some(Active::HoldToButton(active));
                    return Vec::new();
                }
                if self.keys.is_physically_held(active.key) {
                    // Threshold reached and the key is still held: arm and wait for
                    // the button down. No command yet.
                    active.armed = true;
                    self.active = Some(Active::HoldToButton(active));
                    Vec::new()
                } else {
                    // Released before the deadline: replay the held events.
                    let replay = self.pending.take_replay();
                    self.mark_replay_seen(&replay);
                    if replay.is_empty() {
                        Vec::new()
                    } else {
                        vec![Command::Replay { events: replay }]
                    }
                }
            }
            _ => Vec::new(),
        }
    }

    /// The next deadline that should trigger [`Matcher::on_timeout`], if any.
    pub fn next_deadline(&self) -> Option<u64> {
        match &self.active {
            Some(Active::Holding(hold)) => Some(hold.deadline_ms),
            Some(Active::HoldToButton(hb)) if !hb.armed => Some(hb.deadline_ms),
            _ => None,
        }
    }

    /// Drive any due deadline, returning the resulting commands. Returns nothing
    /// when there is no deadline or it has not yet elapsed. Used by the platform
    /// timer so the hook thread does not take/reput the active state needlessly.
    pub fn poll_timeouts(&mut self) -> Vec<Command> {
        let Some(deadline) = self.next_deadline() else {
            return Vec::new();
        };
        if self.clock.now_ms() < deadline {
            return Vec::new();
        }
        self.on_timeout()
    }

    /// Pause or resume matching. Pausing stops new holds/chords and returns any
    /// held events so the caller can flush them before switching to pass-through.
    pub fn set_paused(&mut self, paused: bool) -> Vec<InputEvent> {
        self.paused = paused;
        if paused {
            let held = self.pending.take_replay();
            self.active = None;
            // Clear all tracking. A consumed key/button released after pause then
            // passes through as an orphan up, which is harmless (applications
            // ignore an up without a prior down and no click/character is
            // generated). Replaying a synthetic *down* instead would re-trigger
            // clicks/keystrokes, so it is deliberately not done (M6 P2 #6).
            self.keys.clear();
            self.buttons.clear();
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

    /// Set or clear the internal overflow-bypass flag. Setting it true also
    /// clears any active prefix and state; clearing it recovers after an
    /// overflow once the caller re-arms the matcher (e.g. on resume).
    pub fn set_bypassed(&mut self, bypassed: bool) {
        if bypassed {
            self.enter_bypass();
        } else {
            self.bypassed = false;
        }
    }

    fn on_key_event(&mut self, event: InputEvent) -> (Decision, Resolution) {
        let key = event.key().expect("keyboard event carries a key");
        let (active_first, is_hold_like) = match &self.active {
            Some(Active::Holding(hold)) => (Some(hold.key), true),
            Some(Active::HoldToButton(hb)) => (Some(hb.key), true),
            Some(Active::Chording(chord)) => (Some(chord.first), false),
            None => (None, false),
        };

        match active_first {
            Some(first) if first == key => self.on_first_key_event(key, event),
            Some(_) if is_hold_like => {
                // A non-matching key during a hold-like prefix fails the hold so
                // the held key and this key are replayed in their original order
                // (preserving combinations such as Ctrl+A while a Hold or
                // Hold+MouseButton prefix is pending). Releases still pass
                // through, as do auto-repeat downs of an already-passed key.
                if event.is_key_down() && !event.is_repeat() {
                    self.fail_active(event)
                } else {
                    self.pass_through(event)
                }
            }
            Some(first) => self.on_chord_second_key(first, key, event),
            None => self.on_idle_key(key, event),
        }
    }

    fn on_button_event(&mut self, event: InputEvent) -> (Decision, Resolution) {
        let button = event.button().expect("button event carries a button");
        let (chord_first, hold_button) = match &self.active {
            Some(Active::Chording(chord)) => (Some(chord.first), None),
            Some(Active::HoldToButton(hb)) => (None, Some(hb.clone())),
            _ => (None, None),
        };
        if let Some(first) = chord_first {
            self.on_chord_button(first, button, event)
        } else if let Some(hb) = hold_button {
            self.on_hold_button(hb, button, event)
        } else {
            self.pass_through(event)
        }
    }

    fn on_idle_key(&mut self, key: Key, event: InputEvent) -> (Decision, Resolution) {
        // Only a non-repeat key-down can start a prefix.
        if event.is_key_down() && !event.is_repeat() {
            if let Some((id, timeout_ms, action)) = self.index.hold(key).cloned() {
                return self.start_hold(key, event, id, timeout_ms, action);
            }
            if let Some((id, timeout_ms, button, action)) = self.index.hold_button(key).cloned() {
                return self.start_hold_button(key, button, event, id, timeout_ms, action);
            }
            if self.index.is_first_candidate(key) {
                return self.start_chord(key, event);
            }
        }
        self.pass_through(event)
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
            return self.overflow_flush();
        }
        self.keys.mark_physical_down(key);
        let deadline_ms = self.clock.now_ms() + timeout_ms;
        self.active = Some(Active::Holding(ActiveHold {
            rule_id: id,
            key,
            deadline_ms,
            action,
        }));
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Pending,
        )
    }

    fn start_hold_button(
        &mut self,
        key: Key,
        button: MouseButton,
        event: InputEvent,
        id: String,
        timeout_ms: u64,
        action: Action,
    ) -> (Decision, Resolution) {
        if self.pending.push(event).is_err() {
            return self.overflow_flush();
        }
        self.keys.mark_physical_down(key);
        let deadline_ms = self.clock.now_ms() + timeout_ms;
        self.active = Some(Active::HoldToButton(ActiveHoldButton {
            rule_id: id,
            key,
            button,
            deadline_ms,
            armed: false,
            action,
        }));
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Pending,
        )
    }

    fn start_chord(&mut self, key: Key, event: InputEvent) -> (Decision, Resolution) {
        if self.pending.push(event).is_err() {
            return self.overflow_flush();
        }
        self.keys.mark_physical_down(key);
        self.active = Some(Active::Chording(ActiveChord { first: key }));
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Pending,
        )
    }

    /// Handle a key event belonging to the active first key (Hold or Chord).
    fn on_first_key_event(&mut self, key: Key, event: InputEvent) -> (Decision, Resolution) {
        if event.is_key_up() {
            // First key released before completion: fail and replay in order.
            self.keys.mark_physical_up(key);
            if self.pending.push(event).is_err() {
                return self.overflow_flush();
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

        // Auto-repeat down while the prefix is active: absorb it. The original
        // down is already held in `pending`; re-adding repeats would only burn
        // queue capacity (and eventually overflow) and must not reset the timer
        // (M5/M6 semantics).
        if event.is_repeat() {
            return (
                Decision::Suppress {
                    event_id: event.seq,
                },
                Resolution::Pending,
            );
        }

        // Stray (non-repeat) down while the prefix is active: keep holding. The
        // timer is NOT reset (M5 semantics).
        if self.pending.push(event).is_err() {
            return self.overflow_flush();
        }
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Pending,
        )
    }

    fn on_chord_second_key(
        &mut self,
        first: Key,
        second: Key,
        event: InputEvent,
    ) -> (Decision, Resolution) {
        if event.is_key_down() && !event.is_repeat() {
            if let Some((rule_id, action)) = self.index.second_key(first, second).cloned() {
                return self.match_chord(rule_id, action, event, first, Some(second), None);
            }
            return self.fail_active(event);
        }
        self.pass_through(event)
    }

    fn on_chord_button(
        &mut self,
        first: Key,
        button: MouseButton,
        event: InputEvent,
    ) -> (Decision, Resolution) {
        if event.is_button_down() {
            if let Some((rule_id, action)) = self.index.second_button(first, button).cloned() {
                return self.match_chord(rule_id, action, event, first, None, Some(button));
            }
            return self.fail_active(event);
        }
        self.pass_through(event)
    }

    /// Handle a mouse-button event while a `Hold+MouseButton` rule is active.
    /// Any button down is decisive: if the rule is armed (threshold reached with
    /// the key still held) and it is the rule's button, match and consume;
    /// otherwise fail and replay (an early button is not allowed in the MVP).
    fn on_hold_button(
        &mut self,
        active: ActiveHoldButton,
        button: MouseButton,
        event: InputEvent,
    ) -> (Decision, Resolution) {
        if event.is_button_down() {
            if active.armed && button == active.button {
                return self.match_hold_button(active, event);
            }
            return self.fail_active(event);
        }
        self.pass_through(event)
    }

    /// Consume the held key and the completing button, and emit the action once.
    fn match_hold_button(
        &mut self,
        active: ActiveHoldButton,
        event: InputEvent,
    ) -> (Decision, Resolution) {
        // Held key downs are consumed, not replayed.
        self.pending.take_replay();
        self.keys.mark_consumed(active.key);
        self.buttons.mark_consumed(active.button);
        self.active = None;
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Matched {
                rule_id: active.rule_id,
                action: active.action,
            },
        )
    }

    /// A non-matching second input (or an early button) arrived: replay the held
    /// first-key events followed by this input, in order, so the target sees the
    /// original order.
    fn fail_active(&mut self, event: InputEvent) -> (Decision, Resolution) {
        if self.pending.push(event).is_err() {
            return self.overflow_flush();
        }
        let replay = self.pending.take_replay();
        self.active = None;
        self.mark_replay_seen(&replay);
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Failed { replay },
        )
    }

    /// Consume the trigger inputs and emit the action once.
    fn match_chord(
        &mut self,
        rule_id: String,
        action: Action,
        event: InputEvent,
        first: Key,
        second_key: Option<Key>,
        second_button: Option<MouseButton>,
    ) -> (Decision, Resolution) {
        // The held first-key downs are consumed, not replayed.
        self.pending.take_replay();
        self.keys.mark_consumed(first);
        if let Some(key) = second_key {
            self.keys.mark_consumed(key);
        }
        if let Some(button) = second_button {
            self.buttons.mark_consumed(button);
        }
        self.active = None;
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Matched { rule_id, action },
        )
    }

    fn consume_key(&mut self, key: Key, event: InputEvent) -> (Decision, Resolution) {
        if event.is_key_up() {
            self.keys.clear_consumed(key);
            self.keys.mark_physical_up(key);
        }
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Pending,
        )
    }

    fn consume_button(&mut self, button: MouseButton, event: InputEvent) -> (Decision, Resolution) {
        if event.is_button_up() {
            self.buttons.clear_consumed(button);
            self.buttons.mark_physical_up(button);
        }
        (
            Decision::Suppress {
                event_id: event.seq,
            },
            Resolution::Pending,
        )
    }

    fn pass_through(&mut self, event: InputEvent) -> (Decision, Resolution) {
        match event.source {
            InputSource::Keyboard {
                key, down, repeat, ..
            } => {
                if down {
                    if !repeat {
                        self.keys.mark_physical_down(key);
                        self.keys.mark_seen_down(key);
                    }
                } else {
                    self.keys.mark_physical_up(key);
                    self.keys.mark_seen_up(key);
                }
            }
            InputSource::Mouse {
                kind: MouseKind::ButtonDown(button),
                ..
            } => {
                self.buttons.mark_physical_down(button);
                self.buttons.mark_seen_down(button);
            }
            InputSource::Mouse {
                kind: MouseKind::ButtonUp(button),
                ..
            } => {
                self.buttons.mark_physical_up(button);
                self.buttons.mark_seen_up(button);
            }
            InputSource::Mouse { .. } => {}
        }
        (Decision::PassThrough, Resolution::Pending)
    }

    fn overflow_flush(&mut self) -> (Decision, Resolution) {
        self.enter_bypass();
        let replay = self.pending.take_replay();
        (Decision::PassThrough, Resolution::Failed { replay })
    }

    fn enter_bypass(&mut self) {
        self.bypassed = true;
        self.active = None;
        self.keys.clear();
        self.buttons.clear();
    }

    fn mark_replay_seen(&mut self, events: &[InputEvent]) {
        for event in events {
            match event.source {
                InputSource::Keyboard {
                    key, down, repeat, ..
                } => {
                    if down {
                        if !repeat {
                            self.keys.mark_seen_down(key);
                        }
                    } else {
                        self.keys.mark_seen_up(key);
                    }
                }
                InputSource::Mouse {
                    kind: MouseKind::ButtonDown(button),
                    ..
                } => {
                    self.buttons.mark_seen_down(button);
                }
                InputSource::Mouse {
                    kind: MouseKind::ButtonUp(button),
                    ..
                } => {
                    self.buttons.mark_seen_up(button);
                }
                InputSource::Mouse { .. } => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseButton;
    use crate::rules::{Rule, RuleIndex, Trigger};

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

    fn button(seq: u64, time_ms: u64, button: MouseButton, down: bool) -> InputEvent {
        InputEvent {
            seq,
            time_ms,
            injected: false,
            source: InputSource::Mouse {
                kind: if down {
                    MouseKind::ButtonDown(button)
                } else {
                    MouseKind::ButtonUp(button)
                },
                x: 0,
                y: 0,
            },
        }
    }

    fn hold_rule(key: Key, timeout_ms: u64) -> Rule {
        Rule {
            id: "hold".to_string(),
            trigger: Trigger::Hold { key, timeout_ms },
            action: Action::KeyChord(vec![Key::C]),
        }
    }

    fn chord_rule(id: &str, first: Key, second: Key) -> Rule {
        Rule {
            id: id.to_string(),
            trigger: Trigger::KeyChord { first, second },
            action: Action::KeyChord(vec![Key::C]),
        }
    }

    fn mouse_chord_rule(id: &str, key: Key, button: MouseButton) -> Rule {
        Rule {
            id: id.to_string(),
            trigger: Trigger::KeyMouseButton { key, button },
            action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
        }
    }

    fn matcher_with(clock: ManualClock, rules: Vec<Rule>, capacity: usize) -> Matcher {
        let index = RuleIndex::compile(rules).expect("valid rules");
        Matcher::new(Box::new(clock), index, capacity)
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
        assert_eq!(m.next_deadline(), Some(100));

        clock.advance(10);
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
    fn hold_fails_on_other_key_down_preserving_chord() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::LeftCtrl, 250)], 8);

        let ctrl = down(0, 0, Key::LeftCtrl);
        m.on_event(ctrl);

        // While the hold prefix is pending, a non-matching key down fails the
        // hold and replays [Ctrl, A] in order, preserving Ctrl+A (M6 P1 #3).
        let a = down(1, 10, Key::A);
        let (decision, resolution) = m.on_event(a);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(resolution, Resolution::Failed { replay: vec![ctrl, a] });
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn hold_button_fails_on_other_key_down_preserving_chord() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![hold_button_rule(Key::LeftCtrl, 250, MouseButton::Right)],
            8,
        );

        let ctrl = down(0, 0, Key::LeftCtrl);
        m.on_event(ctrl);

        // Same as the single-key Hold case: a non-matching key must fail the
        // hold-like prefix and replay in order (preserve Ctrl+A).
        let a = down(1, 10, Key::A);
        let (decision, resolution) = m.on_event(a);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(resolution, Resolution::Failed { replay: vec![ctrl, a] });
    }

    #[test]
    fn pause_does_not_replay_consumed_inputs() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![hold_button_rule(Key::LeftCtrl, 250, MouseButton::Right)],
            8,
        );

        // Complete a Hold+MouseButton match: Ctrl held to threshold, then the
        // right button completes it. Both downs are consumed (not delivered).
        m.on_event(down(0, 0, Key::LeftCtrl));
        clock.advance(250);
        m.on_timeout(); // arm the rule (Ctrl still held)
        let (_, resolution) = m.on_event(button(1, 250, MouseButton::Right, true));
        assert!(matches!(
            resolution,
            Resolution::Matched { ref rule_id, .. } if rule_id == "hold-click"
        ));

        // Pausing must NOT replay synthetic downs (which would re-trigger a click
        // or keystroke); it clears state so the later releases are harmless
        // orphan ups (M6 P2 #6 / round-2 C).
        assert!(m.set_paused(true).is_empty());

        // Releases now pass through (harmless orphan ups), never suppressed.
        assert_eq!(
            m.on_event(button(2, 260, MouseButton::Right, false)).0,
            Decision::PassThrough
        );
        assert_eq!(
            m.on_event(up(3, 261, Key::LeftCtrl)).0,
            Decision::PassThrough
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
    fn mouse_button_passes_through_without_active_chord() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![mouse_chord_rule("copy", Key::LeftCtrl, MouseButton::Right)],
            8,
        );

        let (decision, _) = m.on_event(button(0, 0, MouseButton::Right, true));
        assert_eq!(decision, Decision::PassThrough);
    }

    #[test]
    fn no_rules_everything_passes_through() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![], 8);

        let (decision, _) = m.on_event(down(0, 0, Key::LeftCtrl));
        assert_eq!(decision, Decision::PassThrough);
        let (decision, _) = m.on_event(button(1, 1, MouseButton::Right, true));
        assert_eq!(decision, Decision::PassThrough);
    }

    #[test]
    fn key_chord_matches_and_consumes_both_inputs() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![chord_rule("quit", Key::LeftCtrl, Key::Q)],
            8,
        );

        let (decision, resolution) = m.on_event(down(0, 0, Key::LeftCtrl));
        assert_eq!(decision, Decision::Suppress { event_id: 0 });
        assert_eq!(resolution, Resolution::Pending);
        assert_eq!(m.next_deadline(), None);

        let q = down(1, 1, Key::Q);
        let (decision, resolution) = m.on_event(q);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Matched {
                rule_id: "quit".to_string(),
                action: Action::KeyChord(vec![Key::C]),
            }
        );

        let (decision, _) = m.on_event(up(2, 2, Key::Q));
        assert_eq!(decision, Decision::Suppress { event_id: 2 });
        let (decision, _) = m.on_event(up(3, 3, Key::LeftCtrl));
        assert_eq!(decision, Decision::Suppress { event_id: 3 });
    }

    #[test]
    fn key_chord_fails_when_first_released() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![chord_rule("quit", Key::LeftCtrl, Key::Q)],
            8,
        );

        let d0 = down(0, 0, Key::LeftCtrl);
        m.on_event(d0);
        let u0 = up(1, 1, Key::LeftCtrl);
        let (decision, resolution) = m.on_event(u0);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, u0]
            }
        );
    }

    #[test]
    fn key_chord_fails_on_non_matching_second_key() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![chord_rule("quit", Key::LeftCtrl, Key::Q)],
            8,
        );

        let d0 = down(0, 0, Key::LeftCtrl);
        m.on_event(d0);
        let x = down(1, 1, Key::X);
        let (decision, resolution) = m.on_event(x);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, x]
            }
        );
        let (decision, _) = m.on_event(up(2, 2, Key::X));
        assert_eq!(decision, Decision::PassThrough);
        let (decision, _) = m.on_event(up(3, 3, Key::LeftCtrl));
        assert_eq!(decision, Decision::PassThrough);
    }

    #[test]
    fn key_mouse_chord_matches_and_consumes_button() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![mouse_chord_rule("copy", Key::LeftCtrl, MouseButton::Right)],
            8,
        );

        let (decision, resolution) = m.on_event(down(0, 0, Key::LeftCtrl));
        assert_eq!(decision, Decision::Suppress { event_id: 0 });
        assert_eq!(resolution, Resolution::Pending);

        let right = button(1, 1, MouseButton::Right, true);
        let (decision, resolution) = m.on_event(right);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Matched {
                rule_id: "copy".to_string(),
                action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
            }
        );

        let (decision, _) = m.on_event(button(2, 2, MouseButton::Right, false));
        assert_eq!(decision, Decision::Suppress { event_id: 2 });
        let (decision, _) = m.on_event(up(3, 3, Key::LeftCtrl));
        assert_eq!(decision, Decision::Suppress { event_id: 3 });
    }

    #[test]
    fn key_mouse_chord_fails_on_non_matching_button() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![mouse_chord_rule("copy", Key::LeftCtrl, MouseButton::Right)],
            8,
        );

        let d0 = down(0, 0, Key::LeftCtrl);
        m.on_event(d0);
        let left = button(1, 1, MouseButton::Left, true);
        let (decision, resolution) = m.on_event(left);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, left]
            }
        );
    }

    #[test]
    fn multi_candidate_rules_share_first_key() {
        let clock = ManualClock::new(0);
        let rules = vec![
            chord_rule("quit", Key::LeftCtrl, Key::Q),
            mouse_chord_rule("copy", Key::LeftCtrl, MouseButton::Right),
        ];

        let mut m = matcher_with(clock.clone(), rules.clone(), 8);
        m.on_event(down(0, 0, Key::LeftCtrl));
        let (_, resolution) = m.on_event(down(1, 1, Key::Q));
        assert_eq!(
            resolution,
            Resolution::Matched {
                rule_id: "quit".to_string(),
                action: Action::KeyChord(vec![Key::C]),
            }
        );

        let mut m2 = matcher_with(clock.clone(), rules, 8);
        m2.on_event(down(0, 0, Key::LeftCtrl));
        let (_, resolution) = m2.on_event(button(1, 1, MouseButton::Right, true));
        assert_eq!(
            resolution,
            Resolution::Matched {
                rule_id: "copy".to_string(),
                action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
            }
        );
    }

    fn hold_button_rule(key: Key, timeout_ms: u64, button: MouseButton) -> Rule {
        Rule {
            id: "hold-click".to_string(),
            trigger: Trigger::HoldMouseButton {
                key,
                timeout_ms,
                button,
            },
            action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
        }
    }

    #[test]
    fn hold_mouse_button_arms_at_threshold_then_matches() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![hold_button_rule(Key::LeftCtrl, 250, MouseButton::Right)],
            8,
        );

        assert_eq!(
            m.on_event(down(0, 0, Key::LeftCtrl)).0,
            Decision::Suppress { event_id: 0 }
        );
        assert_eq!(m.next_deadline(), Some(250));

        clock.advance(250);
        assert!(m.on_timeout().is_empty());
        assert_eq!(m.next_deadline(), None);

        let right = button(1, 250, MouseButton::Right, true);
        let (decision, resolution) = m.on_event(right);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Matched {
                rule_id: "hold-click".to_string(),
                action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
            }
        );

        assert_eq!(
            m.on_event(button(2, 251, MouseButton::Right, false)).0,
            Decision::Suppress { event_id: 2 }
        );
        assert_eq!(
            m.on_event(up(3, 252, Key::LeftCtrl)).0,
            Decision::Suppress { event_id: 3 }
        );
    }

    #[test]
    fn hold_mouse_button_fails_on_button_before_threshold() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![hold_button_rule(Key::LeftCtrl, 250, MouseButton::Right)],
            8,
        );

        let d0 = down(0, 0, Key::LeftCtrl);
        m.on_event(d0);
        let early = button(1, 100, MouseButton::Right, true);
        let (decision, resolution) = m.on_event(early);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, early]
            }
        );

        let (decision, _) = m.on_event(up(2, 101, Key::LeftCtrl));
        assert_eq!(decision, Decision::PassThrough);
    }

    #[test]
    fn hold_mouse_button_fails_on_non_matching_button_after_threshold() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![hold_button_rule(Key::LeftCtrl, 250, MouseButton::Right)],
            8,
        );

        let d0 = down(0, 0, Key::LeftCtrl);
        m.on_event(d0);
        clock.advance(250);
        m.on_timeout();

        let left = button(1, 250, MouseButton::Left, true);
        let (decision, resolution) = m.on_event(left);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, left]
            }
        );
    }

    #[test]
    fn hold_mouse_button_fails_when_key_released_before_threshold() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![hold_button_rule(Key::LeftCtrl, 250, MouseButton::Right)],
            8,
        );

        let d0 = down(0, 0, Key::LeftCtrl);
        m.on_event(d0);
        let u0 = up(1, 100, Key::LeftCtrl);
        let (decision, resolution) = m.on_event(u0);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, u0]
            }
        );
    }

    #[test]
    fn hold_mouse_button_fails_when_key_released_after_arming() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![hold_button_rule(Key::LeftCtrl, 250, MouseButton::Right)],
            8,
        );

        let d0 = down(0, 0, Key::LeftCtrl);
        m.on_event(d0);
        clock.advance(250);
        assert!(m.on_timeout().is_empty());

        let u0 = up(1, 250, Key::LeftCtrl);
        let (decision, resolution) = m.on_event(u0);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, u0]
            }
        );
    }

    #[test]
    fn hold_mouse_button_repeat_does_not_reset_timer() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(
            clock.clone(),
            vec![hold_button_rule(Key::LeftCtrl, 250, MouseButton::Right)],
            8,
        );

        m.on_event(down(0, 0, Key::LeftCtrl));
        clock.advance(240);
        let r = repeat(1, 240, Key::LeftCtrl);
        let (decision, _) = m.on_event(r);
        assert_eq!(decision, Decision::Suppress { event_id: 1 });
        assert_eq!(m.next_deadline(), Some(250));

        clock.advance(10);
        assert!(m.on_timeout().is_empty());
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn hold_exactly_at_threshold_matches() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        m.on_event(down(0, 0, Key::F8));
        clock.advance(100);
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
    fn hold_released_just_before_threshold_replays() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        let d0 = down(0, 0, Key::F8);
        m.on_event(d0);
        clock.advance(99);
        let u0 = up(1, 99, Key::F8);
        let (_, resolution) = m.on_event(u0);
        assert_eq!(
            resolution,
            Resolution::Failed {
                replay: vec![d0, u0]
            }
        );
    }

    #[test]
    fn poll_timeouts_only_fires_when_due() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 8);

        m.on_event(down(0, 0, Key::F8));
        clock.advance(50);
        assert!(m.poll_timeouts().is_empty());

        clock.advance(50);
        assert_eq!(
            m.poll_timeouts(),
            vec![Command::Emit {
                rule_id: "hold".to_string(),
                action: Action::KeyChord(vec![Key::C]),
            }]
        );
    }

    #[test]
    fn overflow_bypasses_and_flushes_held_events() {
        let clock = ManualClock::new(0);
        let mut m = matcher_with(clock.clone(), vec![hold_rule(Key::F8, 100)], 1);

        let d0 = down(0, 0, Key::F8);
        assert_eq!(m.on_event(d0).0, Decision::Suppress { event_id: 0 });

        let d1 = down(1, 1, Key::F8);
        let (decision, resolution) = m.on_event(d1);
        assert_eq!(decision, Decision::PassThrough);
        assert_eq!(resolution, Resolution::Failed { replay: vec![d0] });
        assert!(m.is_bypassed());

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

        m.set_paused(false);
        let (decision, _) = m.on_event(down(2, 20, Key::F8));
        assert_eq!(decision, Decision::Suppress { event_id: 2 });
    }
}
