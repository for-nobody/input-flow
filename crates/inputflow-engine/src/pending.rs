//! Bounded pending (hold) queue.
//!
//! Holds events that have been suppressed while the matcher decides whether
//! they form a rule. The queue is bounded: once it reaches capacity it flags an
//! overflow, after which the caller is expected to best-effort flush (replay)
//! the already-held events in order and stop intercepting new events.

use std::collections::VecDeque;

use crate::event::InputEvent;

/// Error returned when an event cannot be held because the queue is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overflow;

/// A bounded FIFO queue of suppressed events awaiting a decision.
pub struct PendingQueue {
    capacity: usize,
    entries: VecDeque<InputEvent>,
    overflow: bool,
}

impl PendingQueue {
    /// Create an empty queue with the given capacity. `capacity == 0` means the
    /// queue cannot hold anything and every [`Self::push`] overflows immediately.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::new(),
            overflow: false,
        }
    }

    /// Try to hold one event. On overflow the event is not held, the queue marks
    /// itself overflowed, and `Err(Overflow)` is returned.
    pub fn push(&mut self, event: InputEvent) -> Result<(), Overflow> {
        if self.entries.len() >= self.capacity {
            self.overflow = true;
            return Err(Overflow);
        }
        self.entries.push_back(event);
        Ok(())
    }

    /// Take all held events in FIFO order (the replay plan) and clear the queue.
    pub fn take_replay(&mut self) -> Vec<InputEvent> {
        self.entries.drain(..).collect()
    }

    /// Number of currently held events.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no events are currently held.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the queue ever overflowed since it was last cleared.
    pub fn is_overflow(&self) -> bool {
        self.overflow
    }

    /// Clear held events and reset the overflow flag.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.overflow = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{InputSource, Key};

    fn ev(seq: u64) -> InputEvent {
        InputEvent {
            seq,
            time_ms: 0,
            injected: false,
            source: InputSource::Keyboard {
                key: Key::F8,
                scan_code: 0,
                extended: false,
                down: true,
                repeat: false,
            },
        }
    }

    #[test]
    fn holds_up_to_capacity_in_fifo_order() {
        let mut q = PendingQueue::new(2);
        assert!(q.push(ev(0)).is_ok());
        assert!(q.push(ev(1)).is_ok());
        assert_eq!(q.len(), 2);
        assert_eq!(q.take_replay(), vec![ev(0), ev(1)]);
        assert!(q.is_empty());
    }

    #[test]
    fn overflow_flags_and_rejects_new_events() {
        let mut q = PendingQueue::new(1);
        assert!(q.push(ev(0)).is_ok());
        assert_eq!(q.push(ev(1)), Err(Overflow));
        assert!(q.is_overflow());
        assert_eq!(q.len(), 1);
        assert_eq!(q.take_replay(), vec![ev(0)]);
    }

    #[test]
    fn clear_resets_overflow_flag() {
        let mut q = PendingQueue::new(1);
        q.push(ev(0)).unwrap();
        let _ = q.push(ev(1));
        assert!(q.is_overflow());
        q.clear();
        assert!(!q.is_overflow());
        assert!(q.is_empty());
    }

    #[test]
    fn zero_capacity_always_overflows() {
        let mut q = PendingQueue::new(0);
        assert_eq!(q.push(ev(0)), Err(Overflow));
        assert!(q.is_overflow());
    }
}
