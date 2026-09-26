//! Windows platform integration for InputFlow.
//!
//! Hosts the `unsafe` Win32 code (low-level hooks, the message loop,
//! `SendInput`) and the Win32 virtual-key / mouse-message -> engine mapping.
//! All `unsafe` is confined to [`platform::windows`]; [`keymap`] is pure logic
//! and unit-testable without a hook.

pub mod keymap;
pub mod platform;
