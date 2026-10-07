//! Platform-independent input event model.
//!
//! These types are intentionally free of any Windows dependency. A [`Key`]
//! carries either a logical key identity or an explicit physical scan identity.
//! The mapping from Win32 virtual-key codes to logical keys lives in the Windows
//! integration crate (`inputflow-windows`), not here. `Unknown(u16)` preserves
//! an unmapped raw platform virtual-key code during normalization; it is not a
//! valid stable configuration identity.

use std::fmt;

macro_rules! define_keys {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// A key identity used by rules and events.
        ///
        /// Named variants are logical Windows virtual-key identities. `Physical`
        /// is a layout-independent scan-code identity selected explicitly by a
        /// rule. `Unknown` is an observation-only raw virtual-key fallback.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Key {
            $($variant,)+
            Physical { scan_code: u16, extended: bool },
            Unknown(u16),
        }

        impl Key {
            /// Every named logical key accepted by configuration schema v2.
            pub const NAMED: &'static [Key] = &[$(Key::$variant,)+];

            /// Parse a canonical logical-key name. Physical and unknown
            /// identities must use their explicit schema representations.
            pub fn from_name(name: &str) -> Option<Key> {
                match name {
                    $($name => Some(Key::$variant),)+
                    _ => None,
                }
            }

            /// Build a physical identity. Scan code zero is not stable enough
            /// to configure and therefore has no physical representation.
            pub fn physical(scan_code: u16, extended: bool) -> Option<Key> {
                (scan_code != 0).then_some(Key::Physical { scan_code, extended })
            }
        }

        impl fmt::Display for Key {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Key::$variant => f.write_str($name),)+
                    Key::Physical { scan_code, extended } => write!(
                        f,
                        "Physical(scan=0x{scan_code:02X},extended={extended})"
                    ),
                    Key::Unknown(vk) => write!(f, "Unknown(0x{vk:X})"),
                }
            }
        }
    };
}

define_keys! {
    LeftCtrl => "LeftCtrl",
    RightCtrl => "RightCtrl",
    LeftShift => "LeftShift",
    RightShift => "RightShift",
    LeftAlt => "LeftAlt",
    RightAlt => "RightAlt",
    LeftWin => "LeftWin",
    RightWin => "RightWin",
    A => "A", B => "B", C => "C", D => "D", E => "E", F => "F", G => "G",
    H => "H", I => "I", J => "J", K => "K", L => "L", M => "M", N => "N",
    O => "O", P => "P", Q => "Q", R => "R", S => "S", T => "T", U => "U",
    V => "V", W => "W", X => "X", Y => "Y", Z => "Z",
    Digit0 => "Digit0", Digit1 => "Digit1", Digit2 => "Digit2", Digit3 => "Digit3",
    Digit4 => "Digit4", Digit5 => "Digit5", Digit6 => "Digit6", Digit7 => "Digit7",
    Digit8 => "Digit8", Digit9 => "Digit9",
    F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5", F6 => "F6",
    F7 => "F7", F8 => "F8", F9 => "F9", F10 => "F10", F11 => "F11", F12 => "F12",
    F13 => "F13", F14 => "F14", F15 => "F15", F16 => "F16", F17 => "F17",
    F18 => "F18", F19 => "F19", F20 => "F20", F21 => "F21", F22 => "F22",
    F23 => "F23", F24 => "F24",
    Space => "Space",
    Enter => "Enter",
    NumpadEnter => "NumpadEnter",
    Escape => "Escape",
    Tab => "Tab",
    Backspace => "Backspace",
    CapsLock => "CapsLock",
    NumLock => "NumLock",
    ScrollLock => "ScrollLock",
    Left => "Left",
    Right => "Right",
    Up => "Up",
    Down => "Down",
    Home => "Home",
    End => "End",
    PageUp => "PageUp",
    PageDown => "PageDown",
    Insert => "Insert",
    Delete => "Delete",
    Oem1 => "Oem1",
    OemPlus => "OemPlus",
    OemComma => "OemComma",
    OemMinus => "OemMinus",
    OemPeriod => "OemPeriod",
    Oem2 => "Oem2",
    Oem3 => "Oem3",
    Oem4 => "Oem4",
    Oem5 => "Oem5",
    Oem6 => "Oem6",
    Oem7 => "Oem7",
    Oem8 => "Oem8",
    Oem102 => "Oem102",
    Numpad0 => "Numpad0",
    Numpad1 => "Numpad1",
    Numpad2 => "Numpad2",
    Numpad3 => "Numpad3",
    Numpad4 => "Numpad4",
    Numpad5 => "Numpad5",
    Numpad6 => "Numpad6",
    Numpad7 => "Numpad7",
    Numpad8 => "Numpad8",
    Numpad9 => "Numpad9",
    NumpadMultiply => "NumpadMultiply",
    NumpadAdd => "NumpadAdd",
    NumpadSeparator => "NumpadSeparator",
    NumpadSubtract => "NumpadSubtract",
    NumpadDecimal => "NumpadDecimal",
    NumpadDivide => "NumpadDivide",
    PrintScreen => "PrintScreen",
    Pause => "Pause",
    Apps => "Apps",
    BrowserBack => "BrowserBack",
    BrowserForward => "BrowserForward",
    BrowserRefresh => "BrowserRefresh",
    BrowserStop => "BrowserStop",
    BrowserSearch => "BrowserSearch",
    BrowserFavorites => "BrowserFavorites",
    BrowserHome => "BrowserHome",
    VolumeMute => "VolumeMute",
    VolumeDown => "VolumeDown",
    VolumeUp => "VolumeUp",
    MediaNextTrack => "MediaNextTrack",
    MediaPreviousTrack => "MediaPreviousTrack",
    MediaStop => "MediaStop",
    MediaPlayPause => "MediaPlayPause",
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

/// A normalized mouse event kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseKind {
    /// Cursor movement. Movement is always passed through; rules may only
    /// observe its screen coordinates.
    Move,
    ButtonDown(MouseButton),
    ButtonUp(MouseButton),
    Wheel {
        delta: i32,
    },
    HorizontalWheel {
        delta: i32,
    },
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

    /// The physical scan-code identity, when the hook supplied a nonzero scan
    /// code. This identity is independent of the current keyboard layout.
    pub fn physical_key(&self) -> Option<Key> {
        match self.source {
            InputSource::Keyboard {
                scan_code,
                extended,
                ..
            } => Key::physical(scan_code, extended),
            InputSource::Mouse { .. } => None,
        }
    }

    /// Identity used for held/repeat/release-tombstone state. Prefer the
    /// physical identity so a layout change cannot strand a consumed down; use
    /// the logical identity only when the platform reported no scan code.
    pub fn tracking_key(&self) -> Option<Key> {
        self.physical_key().or_else(|| self.key())
    }

    /// Candidate identities in deterministic rule-selection order. An exact
    /// physical rule takes precedence over a logical rule for the same event.
    pub fn key_identities(&self) -> [Option<Key>; 2] {
        [self.physical_key(), self.key()]
    }

    /// Whether this keyboard event matches a configured logical or physical
    /// identity.
    pub fn matches_key(&self, identity: Key) -> bool {
        match identity {
            Key::Physical { .. } => self.physical_key() == Some(identity),
            _ => self.key() == Some(identity),
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
        for &key in Key::NAMED {
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
        assert_eq!(Key::from_name("Unknown(0xFF)"), None);
        assert_eq!(Key::from_name("Physical(scan=0x1E,extended=false)"), None);
        assert_eq!(MouseButton::from_name("NotAButton"), None);
    }

    #[test]
    fn keyboard_event_exposes_physical_then_logical_identity() {
        let event = InputEvent {
            seq: 7,
            time_ms: 10,
            injected: false,
            source: InputSource::Keyboard {
                key: Key::A,
                scan_code: 0x1e,
                extended: false,
                down: true,
                repeat: false,
            },
        };
        let physical = Key::Physical {
            scan_code: 0x1e,
            extended: false,
        };
        assert_eq!(event.key_identities(), [Some(physical), Some(Key::A)]);
        assert_eq!(event.tracking_key(), Some(physical));
        assert!(event.matches_key(physical));
        assert!(event.matches_key(Key::A));
        assert!(!event.matches_key(Key::B));
    }

    #[test]
    fn zero_scan_code_falls_back_to_logical_tracking() {
        let event = kbd(Key::MediaPlayPause, true, false);
        assert_eq!(event.physical_key(), None);
        assert_eq!(event.tracking_key(), Some(Key::MediaPlayPause));
        assert_eq!(event.key_identities(), [None, Some(Key::MediaPlayPause)]);
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
