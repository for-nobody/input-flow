//! Normalized input events and human-readable rendering.
//!
//! This module is a read-only probe helper: it turns already-decoded fields from
//! the Win32 low-level hook structures (`KBDLLHOOKSTRUCT` / `MSLLHOOKSTRUCT`) into
//! a small, printable representation. It performs no Win32 calls and contains no
//! `unsafe`; the unsafe dereference happens in `platform::windows`.
//!
//! Note: the key-name table references `windows-sys` VK constants for readability.
//! When the engine is extracted into a Windows-free crate in M3, this table will be
//! replaced by a platform-independent key type.

use std::borrow::Cow;
use std::fmt;

use windows_sys::Win32::UI::Input::KeyboardAndMouse as kb;

/// Unique `dwExtraInfo` tag written into input synthesized by this program, so
/// the low-level hook can recognize its own injected events and pass them
/// through without re-suppressing them (avoiding self-triggered recursion).
pub const SELF_EXTRA_INFO_TAG: usize = 0x494E_5055; // ASCII "INPU"

/// Virtual-key code of the key this probe holds and replays (F8).
pub const HELD_KEY_VK: u16 = kb::VK_F8;

/// Virtual-key code of the emergency bypass toggle (F12).
pub const EMERGENCY_KEY_VK: u16 = kb::VK_F12;

/// True when `extra_info` carries this program's own `dwExtraInfo` tag.
pub fn is_own_event(extra_info: usize) -> bool {
    extra_info == SELF_EXTRA_INFO_TAG
}

/// A message passed from the hook callbacks to the logger thread.
#[derive(Debug, Clone, Copy)]
pub enum HookMessage {
    /// A normalized input event to be logged.
    Input(InputEvent),
    /// The bypass state changed to the enclosed value.
    Bypass(bool),
}

/// One observed input event, normalized from the hook structures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Keyboard {
        seq: u64,
        time_ms: u32,
        up: bool,
        vk: u16,
        scan: u16,
        extended: bool,
        repeat: bool,
        injected: bool,
        extra_info: usize,
    },
    Mouse {
        seq: u64,
        time_ms: u32,
        kind: MouseKind,
        x: i32,
        y: i32,
        injected: bool,
        extra_info: usize,
    },
}

/// Mouse event kind, after filtering out mouse-move noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    LeftDown,
    LeftUp,
    RightDown,
    RightUp,
    MiddleDown,
    MiddleUp,
    XButton1Down,
    XButton1Up,
    XButton2Down,
    XButton2Up,
    Wheel { delta: i32 },
    HorizontalWheel { delta: i32 },
}

impl fmt::Display for InputEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputEvent::Keyboard {
                seq,
                time_ms,
                up,
                vk,
                scan,
                extended,
                repeat,
                injected,
                extra_info,
            } => {
                let phase = if *up { "Up" } else { "Down" };
                write!(
                    f,
                    "[seq={seq:06}] t={time_ms}ms kbd {phase} {} vk=0x{vk:02X} scan=0x{scan:02X} ext={extended} repeat={repeat} injected={injected} extra=0x{extra_info:X}",
                    key_name(*vk)
                )
            }
            InputEvent::Mouse {
                seq,
                time_ms,
                kind,
                x,
                y,
                injected,
                extra_info,
            } => {
                write!(
                    f,
                    "[seq={seq:06}] t={time_ms}ms mouse {kind} ({x},{y}) injected={injected} extra=0x{extra_info:X}"
                )
            }
        }
    }
}

impl fmt::Display for MouseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MouseKind::LeftDown => write!(f, "LeftDown"),
            MouseKind::LeftUp => write!(f, "LeftUp"),
            MouseKind::RightDown => write!(f, "RightDown"),
            MouseKind::RightUp => write!(f, "RightUp"),
            MouseKind::MiddleDown => write!(f, "MiddleDown"),
            MouseKind::MiddleUp => write!(f, "MiddleUp"),
            MouseKind::XButton1Down => write!(f, "XButton1Down"),
            MouseKind::XButton1Up => write!(f, "XButton1Up"),
            MouseKind::XButton2Down => write!(f, "XButton2Down"),
            MouseKind::XButton2Up => write!(f, "XButton2Up"),
            MouseKind::Wheel { delta } => write!(f, "Wheel {delta:+.0}"),
            MouseKind::HorizontalWheel { delta } => write!(f, "HWheel {delta:+.0}"),
        }
    }
}

/// Return a human-readable name for a virtual-key code, or `VK_0xNN` when unknown.
pub fn key_name(vk: u16) -> Cow<'static, str> {
    // Printable ASCII letters and digits map directly to their characters.
    if (0x30..=0x39).contains(&vk) || (0x41..=0x5A).contains(&vk) {
        return Cow::Owned((vk as u8 as char).to_string());
    }

    // Function keys F1..F24.
    if (kb::VK_F1..=kb::VK_F24).contains(&vk) {
        return Cow::Owned(format!("F{}", vk - kb::VK_F1 + 1));
    }

    let name = match vk {
        kb::VK_BACK => "Backspace",
        kb::VK_TAB => "Tab",
        kb::VK_RETURN => "Enter",
        kb::VK_ESCAPE => "Escape",
        kb::VK_SPACE => "Space",
        kb::VK_PRIOR => "PageUp",
        kb::VK_NEXT => "PageDown",
        kb::VK_END => "End",
        kb::VK_HOME => "Home",
        kb::VK_LEFT => "Left",
        kb::VK_UP => "Up",
        kb::VK_RIGHT => "Right",
        kb::VK_DOWN => "Down",
        kb::VK_INSERT => "Insert",
        kb::VK_DELETE => "Delete",
        kb::VK_SHIFT => "Shift",
        kb::VK_CONTROL => "Ctrl",
        kb::VK_MENU => "Alt",
        kb::VK_LSHIFT => "LeftShift",
        kb::VK_RSHIFT => "RightShift",
        kb::VK_LCONTROL => "LeftCtrl",
        kb::VK_RCONTROL => "RightCtrl",
        kb::VK_LMENU => "LeftAlt",
        kb::VK_RMENU => "RightAlt",
        kb::VK_LWIN => "LeftWin",
        kb::VK_RWIN => "RightWin",
        kb::VK_CAPITAL => "CapsLock",
        kb::VK_SNAPSHOT => "PrintScreen",
        kb::VK_SCROLL => "ScrollLock",
        kb::VK_PAUSE => "Pause",
        kb::VK_NUMPAD0 => "Numpad0",
        kb::VK_NUMPAD1 => "Numpad1",
        kb::VK_NUMPAD2 => "Numpad2",
        kb::VK_NUMPAD3 => "Numpad3",
        kb::VK_NUMPAD4 => "Numpad4",
        kb::VK_NUMPAD5 => "Numpad5",
        kb::VK_NUMPAD6 => "Numpad6",
        kb::VK_NUMPAD7 => "Numpad7",
        kb::VK_NUMPAD8 => "Numpad8",
        kb::VK_NUMPAD9 => "Numpad9",
        kb::VK_MULTIPLY => "Numpad*",
        kb::VK_ADD => "Numpad+",
        kb::VK_SEPARATOR => "NumpadSeparator",
        kb::VK_SUBTRACT => "Numpad-",
        kb::VK_DECIMAL => "Numpad.",
        kb::VK_DIVIDE => "Numpad/",
        _ => return Cow::Owned(format!("VK_0x{vk:02X}")),
    };

    Cow::Borrowed(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_event_tag_is_recognized() {
        assert!(is_own_event(SELF_EXTRA_INFO_TAG));
        assert!(!is_own_event(0));
        assert!(!is_own_event(0x1234_5678));
    }

    #[test]
    fn held_and_emergency_keys_are_distinct() {
        assert_ne!(HELD_KEY_VK, EMERGENCY_KEY_VK);
    }
}
