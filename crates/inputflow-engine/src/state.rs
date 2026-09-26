//! Physical-held vs seen-by-target key state.
//!
//! Tracks which keys the user is physically holding and which keys the target
//! application has already seen as pressed. This split lets the matcher keep
//! down/up state consistent: a down that was consumed (not delivered to the
//! target) must have its matching up consumed too, so the target never sees an
//! orphan release (NFR-04).

use std::collections::BTreeSet;

use crate::event::Key;

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
}
