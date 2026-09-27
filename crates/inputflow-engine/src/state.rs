//! Physical-held vs seen-by-target key state.
//!
//! Tracks which keys the user is physically holding and which keys the target
//! application has already seen as pressed. This split lets the matcher keep
//! down/up state consistent: a down that was consumed (not delivered to the
//! target) must have its matching up consumed too, so the target never sees an
//! orphan release (NFR-04).

use std::collections::BTreeSet;

use crate::event::{Key, MouseButton};

/// The two key-state models plus the consumed-key tracking.
#[derive(Debug, Default, Clone)]
pub struct KeyState {
    /// Keys the user is currently holding down (physical input).
    physically_held: BTreeSet<Key>,
    /// Keys the target currently believes are down (delivered via pass-through
    /// or replay).
    seen_down: BTreeSet<Key>,
    /// Keys whose down was consumed; their up must also be consumed.
    consumed: BTreeSet<Key>,
}

impl KeyState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark_physical_down(&mut self, key: Key) {
        self.physically_held.insert(key);
    }

    pub fn mark_physical_up(&mut self, key: Key) {
        self.physically_held.remove(&key);
    }

    pub fn is_physically_held(&self, key: Key) -> bool {
        self.physically_held.contains(&key)
    }

    pub fn mark_seen_down(&mut self, key: Key) {
        self.seen_down.insert(key);
    }

    pub fn mark_seen_up(&mut self, key: Key) {
        self.seen_down.remove(&key);
    }

    pub fn is_seen_down(&self, key: Key) -> bool {
        self.seen_down.contains(&key)
    }

    pub fn mark_consumed(&mut self, key: Key) {
        self.consumed.insert(key);
    }

    pub fn is_consumed(&self, key: Key) -> bool {
        self.consumed.contains(&key)
    }

    pub fn clear_consumed(&mut self, key: Key) {
        self.consumed.remove(&key);
    }

    /// Clear transient physical/target tracking while retaining release
    /// tombstones for downs that were consumed by a matched rule.
    pub fn clear_tracking_preserving_consumed(&mut self) {
        self.physically_held.clear();
        self.seen_down.clear();
    }

    pub fn has_consumed(&self) -> bool {
        !self.consumed.is_empty()
    }

    /// Clear all tracking (used when pausing / entering bypass).
    pub fn clear(&mut self) {
        self.physically_held.clear();
        self.seen_down.clear();
        self.consumed.clear();
    }
}

/// Physical-held vs seen-by-target mouse-button state, mirroring [`KeyState`]
/// so `Key+MouseButton` rules can consume/replay button events consistently
/// (NFR-04).
#[derive(Debug, Default, Clone)]
pub struct MouseState {
    /// Buttons the user is currently holding down (physical input).
    physically_held: BTreeSet<MouseButton>,
    /// Buttons the target currently believes are down.
    seen_down: BTreeSet<MouseButton>,
    /// Buttons whose down was consumed; their up must also be consumed.
    consumed: BTreeSet<MouseButton>,
}

impl MouseState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark_physical_down(&mut self, button: MouseButton) {
        self.physically_held.insert(button);
    }

    pub fn mark_physical_up(&mut self, button: MouseButton) {
        self.physically_held.remove(&button);
    }

    pub fn is_physically_held(&self, button: MouseButton) -> bool {
        self.physically_held.contains(&button)
    }

    pub fn mark_seen_down(&mut self, button: MouseButton) {
        self.seen_down.insert(button);
    }

    pub fn mark_seen_up(&mut self, button: MouseButton) {
        self.seen_down.remove(&button);
    }

    pub fn is_seen_down(&self, button: MouseButton) -> bool {
        self.seen_down.contains(&button)
    }

    pub fn mark_consumed(&mut self, button: MouseButton) {
        self.consumed.insert(button);
    }

    pub fn is_consumed(&self, button: MouseButton) -> bool {
        self.consumed.contains(&button)
    }

    pub fn clear_consumed(&mut self, button: MouseButton) {
        self.consumed.remove(&button);
    }

    /// Clear transient physical/target tracking while retaining release
    /// tombstones for downs that were consumed by a matched rule.
    pub fn clear_tracking_preserving_consumed(&mut self) {
        self.physically_held.clear();
        self.seen_down.clear();
    }

    pub fn has_consumed(&self) -> bool {
        !self.consumed.is_empty()
    }

    /// Clear all tracking (used when pausing / entering bypass).
    pub fn clear(&mut self) {
        self.physically_held.clear();
        self.seen_down.clear();
        self.consumed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_physical_seen_and_consumed() {
        let mut s = KeyState::new();

        s.mark_physical_down(Key::A);
        assert!(s.is_physically_held(Key::A));
        s.mark_seen_down(Key::A);
        assert!(s.is_seen_down(Key::A));

        s.mark_consumed(Key::A);
        assert!(s.is_consumed(Key::A));
        s.clear_consumed(Key::A);
        assert!(!s.is_consumed(Key::A));

        s.mark_physical_up(Key::A);
        assert!(!s.is_physically_held(Key::A));
        s.mark_seen_up(Key::A);
        assert!(!s.is_seen_down(Key::A));
    }

    #[test]
    fn clear_resets_everything() {
        let mut s = KeyState::new();
        s.mark_physical_down(Key::A);
        s.mark_seen_down(Key::A);
        s.mark_consumed(Key::A);
        s.clear();
        assert!(!s.is_physically_held(Key::A));
        assert!(!s.is_seen_down(Key::A));
        assert!(!s.is_consumed(Key::A));
    }

    #[test]
    fn transition_clear_preserves_only_consumed_keys() {
        let mut s = KeyState::new();
        s.mark_physical_down(Key::A);
        s.mark_seen_down(Key::A);
        s.mark_consumed(Key::A);

        s.clear_tracking_preserving_consumed();

        assert!(!s.is_physically_held(Key::A));
        assert!(!s.is_seen_down(Key::A));
        assert!(s.is_consumed(Key::A));
        assert!(s.has_consumed());
    }

    #[test]
    fn mouse_state_tracks_physical_seen_and_consumed() {
        let mut s = MouseState::new();

        s.mark_physical_down(MouseButton::Right);
        assert!(s.is_physically_held(MouseButton::Right));
        s.mark_seen_down(MouseButton::Right);
        assert!(s.is_seen_down(MouseButton::Right));

        s.mark_consumed(MouseButton::Right);
        assert!(s.is_consumed(MouseButton::Right));
        s.clear_consumed(MouseButton::Right);
        assert!(!s.is_consumed(MouseButton::Right));

        s.mark_physical_up(MouseButton::Right);
        assert!(!s.is_physically_held(MouseButton::Right));
        s.mark_seen_up(MouseButton::Right);
        assert!(!s.is_seen_down(MouseButton::Right));
    }
}
