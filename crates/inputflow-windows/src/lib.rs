//! Windows platform integration for InputFlow.
//!
//! This crate will host the `unsafe` Win32 code (low-level hooks, the message
//! loop, `SendInput`) and the Win32 virtual-key -> `inputflow-engine::Key`
//! mapping. It is intentionally empty in M3: that milestone focuses on the pure
//! engine (`inputflow-engine`), and the platform integration is migrated here
//! in M4.

pub mod platform;
