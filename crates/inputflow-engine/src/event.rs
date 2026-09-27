//! Platform-independent input event model.
//!
//! These types are intentionally free of any Windows dependency. A [`Key`]
//! carries semantic key identity (with left/right modifiers distinguished); the
//! mapping from Win32 virtual-key codes to [`Key`] lives in the Windows
//! integration crate (`inputflow-windows`), not here. `Unknown(u16)` preserves
//! the raw platform key code for keys that are not modelled yet, so no
//! information is lost during normalization.

use std::fmt;

/// A platform-independent key identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    // Modifiers (left/right distinguished).
    LeftCtrl,
    RightCtrl,
    LeftShift,
    RightShift,
    LeftAlt,
    RightAlt,
    LeftWin,
    RightWin,
    // Letters.
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    // Main-row digits.
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    // Function keys.
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    // Common editing / navigation keys.
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    /// Any key not modelled yet, carrying the raw platform key code.
    Unknown(u16),
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Key::LeftCtrl => "LeftCtrl",
            Key::RightCtrl => "RightCtrl",
            Key::LeftShift => "LeftShift",
            Key::RightShift => "RightShift",
            Key::LeftAlt => "LeftAlt",
            Key::RightAlt => "RightAlt",
            Key::LeftWin => "LeftWin",
            Key::RightWin => "RightWin",
            Key::A => "A",
            Key::B => "B",
            Key::C => "C",
            Key::D => "D",
            Key::E => "E",
            Key::F => "F",
            Key::G => "G",
            Key::H => "H",
            Key::I => "I",
            Key::J => "J",
            Key::K => "K",
            Key::L => "L",
            Key::M => "M",
            Key::N => "N",
            Key::O => "O",
            Key::P => "P",
            Key::Q => "Q",
            Key::R => "R",
            Key::S => "S",
            Key::T => "T",
            Key::U => "U",
            Key::V => "V",
            Key::W => "W",
            Key::X => "X",
            Key::Y => "Y",
            Key::Z => "Z",
            Key::Digit0 => "Digit0",
            Key::Digit1 => "Digit1",
            Key::Digit2 => "Digit2",
            Key::Digit3 => "Digit3",
            Key::Digit4 => "Digit4",
            Key::Digit5 => "Digit5",
            Key::Digit6 => "Digit6",
            Key::Digit7 => "Digit7",
            Key::Digit8 => "Digit8",
            Key::Digit9 => "Digit9",
            Key::F1 => "F1",
            Key::F2 => "F2",
            Key::F3 => "F3",
            Key::F4 => "F4",
            Key::F5 => "F5",
            Key::F6 => "F6",
            Key::F7 => "F7",
            Key::F8 => "F8",
            Key::F9 => "F9",
            Key::F10 => "F10",
            Key::F11 => "F11",
            Key::F12 => "F12",
            Key::F13 => "F13",
            Key::F14 => "F14",
            Key::F15 => "F15",
            Key::F16 => "F16",
            Key::F17 => "F17",
            Key::F18 => "F18",
            Key::F19 => "F19",
            Key::F20 => "F20",
            Key::F21 => "F21",
            Key::F22 => "F22",
            Key::F23 => "F23",
            Key::F24 => "F24",
            Key::Space => "Space",
            Key::Enter => "Enter",
            Key::Escape => "Escape",
            Key::Tab => "Tab",
            Key::Backspace => "Backspace",
            Key::Unknown(vk) => return write!(f, "Unknown(0x{vk:X})"),
        };
        f.write_str(name)
    }
}

impl Key {
    /// Parse a canonical key name (the reverse of [`Key`]'s `Display`). Returns
    /// `None` for unrecognized names, including the `Unknown(..)` fallback, so
    /// config validation can reject unmapped keys instead of accepting them.
    pub fn from_name(name: &str) -> Option<Key> {
        let key = match name {
            "LeftCtrl" => Key::LeftCtrl,
            "RightCtrl" => Key::RightCtrl,
            "LeftShift" => Key::LeftShift,
            "RightShift" => Key::RightShift,
            "LeftAlt" => Key::LeftAlt,
            "RightAlt" => Key::RightAlt,
            "LeftWin" => Key::LeftWin,
            "RightWin" => Key::RightWin,
            "A" => Key::A,
            "B" => Key::B,
            "C" => Key::C,
            "D" => Key::D,
            "E" => Key::E,
            "F" => Key::F,
            "G" => Key::G,
            "H" => Key::H,
            "I" => Key::I,
            "J" => Key::J,
            "K" => Key::K,
            "L" => Key::L,
            "M" => Key::M,
            "N" => Key::N,
            "O" => Key::O,
            "P" => Key::P,
            "Q" => Key::Q,
            "R" => Key::R,
            "S" => Key::S,
            "T" => Key::T,
            "U" => Key::U,
            "V" => Key::V,
            "W" => Key::W,
            "X" => Key::X,
            "Y" => Key::Y,
            "Z" => Key::Z,
            "Digit0" => Key::Digit0,
            "Digit1" => Key::Digit1,
            "Digit2" => Key::Digit2,
            "Digit3" => Key::Digit3,
            "Digit4" => Key::Digit4,
            "Digit5" => Key::Digit5,
            "Digit6" => Key::Digit6,
            "Digit7" => Key::Digit7,
            "Digit8" => Key::Digit8,
            "Digit9" => Key::Digit9,
            "F1" => Key::F1,
            "F2" => Key::F2,
            "F3" => Key::F3,
            "F4" => Key::F4,
            "F5" => Key::F5,
            "F6" => Key::F6,
            "F7" => Key::F7,
            "F8" => Key::F8,
            "F9" => Key::F9,
            "F10" => Key::F10,
            "F11" => Key::F11,
            "F12" => Key::F12,
            "F13" => Key::F13,
            "F14" => Key::F14,
            "F15" => Key::F15,
            "F16" => Key::F16,
            "F17" => Key::F17,
            "F18" => Key::F18,
            "F19" => Key::F19,
            "F20" => Key::F20,
            "F21" => Key::F21,
            "F22" => Key::F22,
            "F23" => Key::F23,
            "F24" => Key::F24,
            "Space" => Key::Space,
            "Enter" => Key::Enter,
            "Escape" => Key::Escape,
            "Tab" => Key::Tab,
            "Backspace" => Key::Backspace,
            _ => return None,
        };
        Some(key)
    }
}

/// A mouse button, distinguished by identity (not by side).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    XButton1,
    XButton2,
}

impl fmt::Display for MouseButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MouseButton::Left => "Left",
            MouseButton::Right => "Right",
            MouseButton::Middle => "Middle",
            MouseButton::XButton1 => "XButton1",
            MouseButton::XButton2 => "XButton2",
        })
    }
}

impl MouseButton {
    /// Parse a canonical mouse-button name (the reverse of [`MouseButton`]'s
    /// `Display`). Returns `None` for unrecognized names.
    pub fn from_name(name: &str) -> Option<MouseButton> {
        let button = match name {
            "Left" => MouseButton::Left,
            "Right" => MouseButton::Right,
            "Middle" => MouseButton::Middle,
            "XButton1" => MouseButton::XButton1,
            "XButton2" => MouseButton::XButton2,
            _ => return None,
        };
        Some(button)
    }
}

/// A mouse event kind, after filtering out mouse-move noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseKind {
    ButtonDown(MouseButton),
    ButtonUp(MouseButton),
    Wheel { delta: i32 },
    HorizontalWheel { delta: i32 },
}

/// The source-specific payload of an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputSource {
    Keyboard {
        key: Key,
        scan_code: u16,
        extended: bool,
        down: bool,
        repeat: bool,
    },
    Mouse {
        kind: MouseKind,
        x: i32,
        y: i32,
    },
}

/// One observed input event.
///
/// NOTE: `injected` is an *origin* hint, not a trusted device identity. The
/// low-level hook cannot reliably attribute an event to a specific physical
/// device; it only reports the injected flag and `dwExtraInfo`. See the project
/// glossary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    /// Monotonic sequence / order id assigned at observation time.
    pub seq: u64,
    /// Monotonic timestamp in milliseconds.
    pub time_ms: u64,
    /// Whether this event was synthesized rather than produced by physical input.
    pub injected: bool,
    /// Keyboard or mouse payload.
    pub source: InputSource,
}

impl InputEvent {
    /// The sequence / order id of this event.
    pub fn id(&self) -> u64 {
        self.seq
    }

    /// The key of this event if it is a keyboard event.
    pub fn key(&self) -> Option<Key> {
        match self.source {
            InputSource::Keyboard { key, .. } => Some(key),
            InputSource::Mouse { .. } => None,
        }
    }

    /// Whether this is a keyboard key-down event.
    pub fn is_key_down(&self) -> bool {
        matches!(self.source, InputSource::Keyboard { down: true, .. })
    }

    /// Whether this is a keyboard key-up event.
    pub fn is_key_up(&self) -> bool {
        matches!(self.source, InputSource::Keyboard { down: false, .. })
    }

    /// Whether this is an auto-repeat key-down event.
    pub fn is_repeat(&self) -> bool {
        matches!(self.source, InputSource::Keyboard { repeat: true, .. })
    }

    /// The mouse button of this event, if it is a button press/release.
    pub fn button(&self) -> Option<MouseButton> {
        match self.source {
            InputSource::Mouse {
                kind: MouseKind::ButtonDown(button),
                ..
            } => Some(button),
            InputSource::Mouse {
                kind: MouseKind::ButtonUp(button),
                ..
            } => Some(button),
            _ => None,
        }
    }

    /// Whether this is a mouse-button-down event.
    pub fn is_button_down(&self) -> bool {
        matches!(
            self.source,
            InputSource::Mouse {
                kind: MouseKind::ButtonDown(_),
                ..
            }
        )
    }

    /// Whether this is a mouse-button-up event.
    pub fn is_button_up(&self) -> bool {
        matches!(
            self.source,
            InputSource::Mouse {
                kind: MouseKind::ButtonUp(_),
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kbd(key: Key, down: bool, repeat: bool) -> InputEvent {
        InputEvent {
            seq: 0,
            time_ms: 0,
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

    #[test]
    fn keyboard_accessors() {
        let d = kbd(Key::A, true, false);
        assert!(d.is_key_down());
        assert!(!d.is_key_up());
        assert!(!d.is_repeat());
        assert_eq!(d.key(), Some(Key::A));
        assert_eq!(d.id(), 0);

        let u = kbd(Key::A, false, false);
        assert!(u.is_key_up());

        let r = kbd(Key::A, true, true);
        assert!(r.is_repeat());
    }

    #[test]
    fn mouse_has_no_key() {
        let m = InputEvent {
            seq: 1,
            time_ms: 0,
            injected: false,
            source: InputSource::Mouse {
                kind: MouseKind::ButtonDown(MouseButton::Left),
                x: 0,
                y: 0,
            },
        };
        assert_eq!(m.key(), None);
        assert!(!m.is_key_down());
    }

    #[test]
    fn key_equality_and_ordering() {
        assert_eq!(Key::A, Key::A);
        assert_ne!(Key::A, Key::B);
        // Ord is required for the BTreeSet-based key-state model.
        assert!(Key::A < Key::B);
        assert_eq!(Key::Unknown(0x77), Key::Unknown(0x77));
    }

    #[test]
    fn key_and_button_names_round_trip() {
        for key in [
            Key::LeftCtrl,
            Key::RightCtrl,
            Key::LeftShift,
            Key::RightShift,
            Key::LeftAlt,
            Key::RightAlt,
            Key::LeftWin,
            Key::RightWin,
            Key::A,
            Key::Z,
            Key::Digit0,
            Key::Digit9,
            Key::F1,
            Key::F12,
            Key::F24,
            Key::Space,
            Key::Enter,
            Key::Escape,
            Key::Tab,
            Key::Backspace,
        ] {
            assert_eq!(Key::from_name(&key.to_string()), Some(key));
        }
        for button in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::XButton1,
            MouseButton::XButton2,
        ] {
            assert_eq!(MouseButton::from_name(&button.to_string()), Some(button));
        }
        assert_eq!(Key::from_name("NotAKey"), None);
        assert_eq!(MouseButton::from_name("NotAButton"), None);
    }

    #[test]
    fn button_accessors() {
        let down = InputEvent {
            seq: 2,
            time_ms: 0,
            injected: false,
            source: InputSource::Mouse {
                kind: MouseKind::ButtonDown(MouseButton::Right),
                x: 0,
                y: 0,
            },
        };
        assert_eq!(down.button(), Some(MouseButton::Right));
        assert!(down.is_button_down());
        assert!(!down.is_button_up());

        let up = InputEvent {
            seq: 3,
            time_ms: 0,
            injected: false,
            source: InputSource::Mouse {
                kind: MouseKind::ButtonUp(MouseButton::Right),
                x: 0,
                y: 0,
            },
        };
        assert!(up.is_button_up());
        assert!(!up.is_button_down());
        assert!(!up.is_repeat());
    }
}
