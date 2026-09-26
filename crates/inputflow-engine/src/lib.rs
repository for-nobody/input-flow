//! InputFlow engine: a platform-independent input matching state machine.
//!
//! This crate contains the pure logic for observing keyboard/mouse events,
//! holding events that could form a rule prefix, matching rules, and deciding
//! whether to pass an event through, suppress it, replay held events, or emit a
//! matched rule's action.
//!
//! It is deliberately free of any Windows dependency so it can be unit-tested
//! deterministically on any platform. All platform integration (low-level
//! hooks, `SendInput`) lives in the `inputflow-windows` crate.

pub mod event;
pub mod matcher;
pub mod pending;
pub mod state;

pub use event::{InputEvent, InputSource, Key, MouseButton, MouseKind};
pub use matcher::{Action, Clock, Command, Decision, ManualClock, Matcher, Resolution, Rule};
pub use pending::{Overflow, PendingQueue};
pub use state::KeyState;
