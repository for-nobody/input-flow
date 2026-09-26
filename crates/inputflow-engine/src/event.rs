//! Platform-independent input event model.
//!
//! These types are intentionally free of any Windows dependency. A [`Key`]
//! carries semantic key identity (with left/right modifiers distinguished); the
//! mapping from Win32 virtual-key codes to [`Key`] lives in the Windows
//! integration crate (`inputflow-windows`), not here. `Unknown(u16)` preserves
//! the raw platform key code for keys that are not modelled yet, so no
//! information is lost during normalization.

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

/// A mouse button, distinguished by identity (not by side).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    XButton1,
    XButton2,
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
}
