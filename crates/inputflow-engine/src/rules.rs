//! Rule model, validation, conflict detection, and precompiled lookup index.
//!
//! Rules are declared in a platform-independent form and precompiled into a
//! [`RuleIndex`] so the matcher's hot path only performs cheap lookups. M4
//! supports `Key+Key` and `Key+MouseButton` chords plus the single-key `Hold`
//! carried over from M3.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::event::{Key, MouseButton};

/// One of the four screen-coordinate mouse movement directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MouseDirection {
    Left,
    Right,
    Up,
    Down,
}

impl MouseDirection {
    const fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Up => 2,
            Self::Down => 3,
        }
    }
}

/// Shared, prevalidated geometry and timing for one activation-key direction
/// group. Distances use screen pixels and time uses the matcher's monotonic
/// clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseDirectionParameters {
    pub min_distance_px: u32,
    pub max_duration_ms: u64,
    pub off_axis_tolerance_px: u32,
}

/// The at-most-four actions belonging to one activation key. This fixed-size
/// representation keeps mouse-move processing independent of total rule count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseDirectionGroup {
    pub parameters: MouseDirectionParameters,
    actions: [Option<(String, Action)>; 4],
}

impl MouseDirectionGroup {
    fn new(parameters: MouseDirectionParameters) -> Self {
        Self {
            parameters,
            actions: std::array::from_fn(|_| None),
        }
    }

    pub fn action(&self, direction: MouseDirection) -> Option<&(String, Action)> {
        self.actions[direction.index()].as_ref()
    }

    fn insert(
        &mut self,
        direction: MouseDirection,
        rule_id: String,
        action: Action,
    ) -> Result<(), RuleError> {
        let slot = &mut self.actions[direction.index()];
        if slot.is_some() {
            return Err(RuleError::DuplicateTrigger(format!(
                "mouse direction {direction:?} for the same activation key"
            )));
        }
        *slot = Some((rule_id, action));
        Ok(())
    }
}

/// Output action carried by a matched rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Synthesize a keyboard chord (all keys pressed together, then released).
    KeyChord(Vec<Key>),
}

/// The trigger condition of a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// Second key pressed while the first key is held down.
    KeyChord { first: Key, second: Key },
    /// Mouse button pressed while the key is held down.
    KeyMouseButton { key: Key, button: MouseButton },
    /// Single key held for at least `timeout_ms`.
    Hold { key: Key, timeout_ms: u64 },
    /// Mouse button pressed after the key has been held for at least `timeout_ms`.
    HoldMouseButton {
        key: Key,
        timeout_ms: u64,
        button: MouseButton,
    },
    /// Mouse net displacement while a keyboard activation key is held.
    MouseDirection {
        key: Key,
        direction: MouseDirection,
        min_distance_px: u32,
        max_duration_ms: u64,
        off_axis_tolerance_px: u32,
    },
}

/// A single enabled rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub trigger: Trigger,
    pub action: Action,
}

/// Why a rule (or a set of rules) was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    DuplicateId(String),
    DuplicateTrigger(String),
    Conflict(String),
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleError::DuplicateId(id) => write!(f, "duplicate rule id `{id}`"),
            RuleError::DuplicateTrigger(desc) => write!(f, "duplicate trigger `{desc}`"),
            RuleError::Conflict(desc) => write!(f, "conflicting rules: {desc}"),
        }
    }
}

fn trigger_desc(trigger: &Trigger) -> String {
    match trigger {
        Trigger::KeyChord { first, second } => format!("{first:?}+{second:?}"),
        Trigger::KeyMouseButton { key, button } => format!("{key:?}+{button:?}"),
        Trigger::Hold { key, timeout_ms } => format!("{key:?} held {timeout_ms}ms"),
        Trigger::HoldMouseButton {
            key,
            timeout_ms,
            button,
        } => format!("{key:?} held {timeout_ms}ms + {button:?}"),
        Trigger::MouseDirection { key, direction, .. } => {
            format!("{key:?} + mouse {direction:?}")
        }
    }
}

/// Precompiled rule index for O(1)-ish lookups from the hot (hook) path.
#[derive(Debug, Clone, Default)]
pub struct RuleIndex {
    /// Every key that can start a `KeyChord` or `KeyMouseButton` rule.
    first_keys: BTreeSet<Key>,
    /// `KeyChord` rules keyed by `first` then `second`.
    chords: BTreeMap<Key, BTreeMap<Key, (String, Action)>>,
    /// `KeyMouseButton` rules keyed by `key` then `button`.
    mouse_chords: BTreeMap<Key, BTreeMap<MouseButton, (String, Action)>>,
    /// `Hold` rules keyed by `key`.
    holds: BTreeMap<Key, (String, u64, Action)>,
    /// `HoldMouseButton` rules keyed by `key` (at most one per key in the MVP).
    hold_buttons: BTreeMap<Key, (String, u64, MouseButton, Action)>,
    /// Mouse direction groups keyed by their keyboard activation identity.
    directions: BTreeMap<Key, MouseDirectionGroup>,
}

impl RuleIndex {
    /// Compile a set of rules, rejecting duplicates. Returns all errors at once.
    pub fn compile(rules: Vec<Rule>) -> Result<Self, Vec<RuleError>> {
        let mut errors = Vec::new();
        let mut seen_ids = BTreeSet::new();
        let mut seen_triggers = BTreeSet::new();
        let mut index = RuleIndex::default();

        for rule in rules {
            if !seen_ids.insert(rule.id.clone()) {
                errors.push(RuleError::DuplicateId(rule.id.clone()));
                continue;
            }
            let desc = trigger_desc(&rule.trigger);
            if !seen_triggers.insert(desc.clone()) {
                errors.push(RuleError::DuplicateTrigger(desc));
                continue;
            }
            match rule.trigger {
                Trigger::KeyChord { first, second } => {
                    index.first_keys.insert(first);
                    index
                        .chords
                        .entry(first)
                        .or_default()
                        .insert(second, (rule.id, rule.action));
                }
                Trigger::KeyMouseButton { key, button } => {
                    index.first_keys.insert(key);
                    index
                        .mouse_chords
                        .entry(key)
                        .or_default()
                        .insert(button, (rule.id, rule.action));
                }
                Trigger::Hold { key, timeout_ms } => match index.holds.entry(key) {
                    Entry::Vacant(entry) => {
                        entry.insert((rule.id, timeout_ms, rule.action));
                    }
                    Entry::Occupied(_) => {
                        errors.push(RuleError::Conflict(format!(
                            "multiple single-key Hold rules share key {key:?}"
                        )));
                    }
                },
                Trigger::HoldMouseButton {
                    key,
                    timeout_ms,
                    button,
                } => match index.hold_buttons.entry(key) {
                    Entry::Vacant(entry) => {
                        entry.insert((rule.id, timeout_ms, button, rule.action));
                    }
                    Entry::Occupied(_) => {
                        errors.push(RuleError::Conflict(format!(
                            "multiple Hold+MouseButton rules share key {key:?}"
                        )));
                    }
                },
                Trigger::MouseDirection {
                    key,
                    direction,
                    min_distance_px,
                    max_duration_ms,
                    off_axis_tolerance_px,
                } => {
                    let parameters = MouseDirectionParameters {
                        min_distance_px,
                        max_duration_ms,
                        off_axis_tolerance_px,
                    };
                    let group = index
                        .directions
                        .entry(key)
                        .or_insert_with(|| MouseDirectionGroup::new(parameters));
                    if group.parameters != parameters {
                        errors.push(RuleError::Conflict(format!(
                            "mouse direction rules sharing activation key {key:?} must use identical distance, duration, and off-axis tolerance"
                        )));
                        continue;
                    }
                    if let Err(error) = group.insert(direction, rule.id, rule.action) {
                        errors.push(error);
                    }
                }
            }
        }

        // Cross-kind prefix conflict (ADR-002): a key may belong to at most one
        // rule kind — single-key Hold, chord group, or Hold+MouseButton. This
        // keeps the single-active-prefix matcher unambiguous.
        let hold_keys: BTreeSet<Key> = index.holds.keys().copied().collect();
        let chord_keys: BTreeSet<Key> = index
            .chords
            .keys()
            .chain(index.mouse_chords.keys())
            .copied()
            .collect();
        let hold_button_keys: BTreeSet<Key> = index.hold_buttons.keys().copied().collect();
        let direction_keys: BTreeSet<Key> = index.directions.keys().copied().collect();

        let mut all_keys: BTreeSet<Key> = BTreeSet::new();
        all_keys.extend(hold_keys.iter().copied());
        all_keys.extend(chord_keys.iter().copied());
        all_keys.extend(hold_button_keys.iter().copied());
        all_keys.extend(direction_keys.iter().copied());

        for key in all_keys {
            let kinds = [
                hold_keys.contains(&key),
                chord_keys.contains(&key),
                hold_button_keys.contains(&key),
                direction_keys.contains(&key),
            ];
            if kinds.iter().filter(|&&present| present).count() > 1 {
                errors.push(RuleError::Conflict(format!(
                    "rules of different kinds share prefix key {key:?}"
                )));
            }
        }

        if errors.is_empty() {
            Ok(index)
        } else {
            Err(errors)
        }
    }

    /// Whether no rules are enabled (everything passes through).
    pub fn is_empty(&self) -> bool {
        self.first_keys.is_empty()
            && self.holds.is_empty()
            && self.hold_buttons.is_empty()
            && self.directions.is_empty()
    }

    /// Whether `key` can start a `KeyChord` or `KeyMouseButton` rule.
    pub fn is_first_candidate(&self, key: Key) -> bool {
        self.first_keys.contains(&key)
    }

    /// The `KeyChord` rule completed by `first` then `second`, if any.
    pub fn second_key(&self, first: Key, second: Key) -> Option<&(String, Action)> {
        self.chords.get(&first).and_then(|map| map.get(&second))
    }

    /// The `KeyMouseButton` rule completed by `key` then `button`, if any.
    pub fn second_button(&self, first: Key, button: MouseButton) -> Option<&(String, Action)> {
        self.mouse_chords
            .get(&first)
            .and_then(|map| map.get(&button))
    }

    /// The `Hold` rule for `key`, if any.
    pub fn hold(&self, key: Key) -> Option<&(String, u64, Action)> {
        self.holds.get(&key)
    }

    /// Whether `key` can start a `HoldMouseButton` rule.
    pub fn has_hold_button(&self, key: Key) -> bool {
        self.hold_buttons.contains_key(&key)
    }

    /// The `HoldMouseButton` rule for `key`, if any.
    pub fn hold_button(&self, key: Key) -> Option<&(String, u64, MouseButton, Action)> {
        self.hold_buttons.get(&key)
    }

    /// The fixed-size mouse direction group for an activation key, if any.
    pub fn mouse_directions(&self, key: Key) -> Option<&MouseDirectionGroup> {
        self.directions.get(&key)
    }

    /// Whether any enabled mouse direction rule exists.
    pub fn has_mouse_directions(&self) -> bool {
        !self.directions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(id: &str, first: Key, second: Key) -> Rule {
        Rule {
            id: id.to_string(),
            trigger: Trigger::KeyChord { first, second },
            action: Action::KeyChord(vec![Key::C]),
        }
    }

    #[test]
    fn compiles_chords_and_looks_up_by_second_input() {
        let index = RuleIndex::compile(vec![
            chord("quit", Key::LeftCtrl, Key::Q),
            Rule {
                id: "copy".to_string(),
                trigger: Trigger::KeyMouseButton {
                    key: Key::LeftCtrl,
                    button: MouseButton::Right,
                },
                action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
            },
        ])
        .unwrap();

        assert!(index.is_first_candidate(Key::LeftCtrl));
        assert!(!index.is_first_candidate(Key::A));
        assert!(!index.is_empty());

        assert_eq!(
            index.second_key(Key::LeftCtrl, Key::Q),
            Some(&("quit".to_string(), Action::KeyChord(vec![Key::C])))
        );
        assert_eq!(index.second_key(Key::LeftCtrl, Key::X), None);
        assert_eq!(
            index.second_button(Key::LeftCtrl, MouseButton::Right),
            Some(&(
                "copy".to_string(),
                Action::KeyChord(vec![Key::LeftCtrl, Key::C])
            ))
        );
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let rules = vec![
            chord("dup", Key::LeftCtrl, Key::Q),
            chord("dup", Key::LeftCtrl, Key::W),
        ];
        let err = RuleIndex::compile(rules).unwrap_err();
        assert_eq!(err, vec![RuleError::DuplicateId("dup".to_string())]);
    }

    #[test]
    fn duplicate_trigger_is_rejected() {
        let rules = vec![
            chord("a", Key::LeftCtrl, Key::Q),
            chord("b", Key::LeftCtrl, Key::Q),
        ];
        let err = RuleIndex::compile(rules).unwrap_err();
        assert_eq!(
            err,
            vec![RuleError::DuplicateTrigger("LeftCtrl+Q".to_string())]
        );
    }

    #[test]
    fn empty_index_is_empty_and_has_no_candidates() {
        let index = RuleIndex::compile(vec![]).unwrap();
        assert!(index.is_empty());
        assert!(!index.is_first_candidate(Key::LeftCtrl));
        assert_eq!(index.hold(Key::F8), None);
        assert!(!index.has_hold_button(Key::LeftCtrl));
    }

    fn hold_button(id: &str, key: Key, button: MouseButton) -> Rule {
        Rule {
            id: id.to_string(),
            trigger: Trigger::HoldMouseButton {
                key,
                timeout_ms: 250,
                button,
            },
            action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
        }
    }

    #[test]
    fn compiles_hold_mouse_button_and_looks_up() {
        let index =
            RuleIndex::compile(vec![hold_button("copy", Key::LeftCtrl, MouseButton::Right)])
                .unwrap();

        assert!(!index.is_empty());
        assert!(index.has_hold_button(Key::LeftCtrl));
        assert!(!index.has_hold_button(Key::A));
        assert_eq!(
            index.hold_button(Key::LeftCtrl),
            Some(&(
                "copy".to_string(),
                250,
                MouseButton::Right,
                Action::KeyChord(vec![Key::LeftCtrl, Key::C])
            ))
        );
    }

    #[test]
    fn single_key_hold_conflicts_with_hold_mouse_button() {
        let rules = vec![
            Rule {
                id: "long-press".to_string(),
                trigger: Trigger::Hold {
                    key: Key::LeftCtrl,
                    timeout_ms: 250,
                },
                action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
            },
            hold_button("hold-click", Key::LeftCtrl, MouseButton::Right),
        ];
        let err = RuleIndex::compile(rules).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(&err[0], RuleError::Conflict(_)));
    }

    #[test]
    fn hold_mouse_button_conflicts_with_chord_on_same_key() {
        let rules = vec![
            chord("quit", Key::LeftCtrl, Key::Q),
            hold_button("hold-click", Key::LeftCtrl, MouseButton::Right),
        ];
        let err = RuleIndex::compile(rules).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(&err[0], RuleError::Conflict(_)));
    }

    #[test]
    fn two_hold_mouse_button_rules_on_same_key_conflict() {
        let rules = vec![
            hold_button("a", Key::LeftCtrl, MouseButton::Right),
            hold_button("b", Key::LeftCtrl, MouseButton::Left),
        ];
        let err = RuleIndex::compile(rules).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(&err[0], RuleError::Conflict(_)));
    }

    fn direction(id: &str, key: Key, direction: MouseDirection, distance: u32) -> Rule {
        Rule {
            id: id.to_string(),
            trigger: Trigger::MouseDirection {
                key,
                direction,
                min_distance_px: distance,
                max_duration_ms: 500,
                off_axis_tolerance_px: 40,
            },
            action: Action::KeyChord(vec![Key::C]),
        }
    }

    #[test]
    fn compiles_fixed_direction_group_and_rejects_duplicates_or_mixed_parameters() {
        let index = RuleIndex::compile(vec![
            direction("left", Key::F8, MouseDirection::Left, 80),
            direction("right", Key::F8, MouseDirection::Right, 80),
        ])
        .unwrap();
        let group = index.mouse_directions(Key::F8).unwrap();
        assert_eq!(group.parameters.min_distance_px, 80);
        assert_eq!(group.action(MouseDirection::Left).unwrap().0, "left");
        assert_eq!(group.action(MouseDirection::Right).unwrap().0, "right");
        assert!(group.action(MouseDirection::Up).is_none());

        let duplicate = RuleIndex::compile(vec![
            direction("one", Key::F8, MouseDirection::Left, 80),
            direction("two", Key::F8, MouseDirection::Left, 120),
        ])
        .unwrap_err();
        assert!(
            duplicate
                .iter()
                .any(|error| matches!(error, RuleError::DuplicateTrigger(_)))
        );

        let mixed = RuleIndex::compile(vec![
            direction("left", Key::F8, MouseDirection::Left, 80),
            direction("up", Key::F8, MouseDirection::Up, 120),
        ])
        .unwrap_err();
        assert!(mixed.iter().any(
            |error| matches!(error, RuleError::Conflict(message) if message.contains("identical"))
        ));
    }

    #[test]
    fn direction_group_conflicts_with_existing_prefix_kinds() {
        let errors = RuleIndex::compile(vec![
            direction("move", Key::F8, MouseDirection::Right, 80),
            Rule {
                id: "hold".to_string(),
                trigger: Trigger::Hold {
                    key: Key::F8,
                    timeout_ms: 250,
                },
                action: Action::KeyChord(vec![Key::C]),
            },
        ])
        .unwrap_err();
        assert!(errors.iter().any(|error| matches!(error, RuleError::Conflict(message) if message.contains("different kinds"))));
    }
}
