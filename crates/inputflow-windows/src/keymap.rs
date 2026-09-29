//! Mapping between Win32 virtual-key codes / low-level-hook metadata and the
//! platform-independent [`inputflow_engine`] types.
//!
//! Pure logic with no `unsafe`; unit-testable without installing a hook. Every
//! keyboard event retains both its logical [`Key`] and the scan-code/extended
//! fields carried by the event itself. [`Key::Unknown`] preserves an unmapped
//! raw virtual-key code for pass-through/replay, but configuration rejects it.

use inputflow_engine::{Key, MouseButton};
use windows_sys::Win32::UI::Input::KeyboardAndMouse as kb;
use windows_sys::Win32::UI::WindowsAndMessaging as wm;

/// Convert low-level-hook keyboard metadata to a logical key identity.
///
/// Most keys are determined by `vk`. Generic modifier VKs use scan/extended
/// metadata to preserve left/right identity. Keypad Enter shares `VK_RETURN`
/// with main Enter and is distinguished by the extended flag.
pub fn hook_key(vk: u16, scan_code: u16, extended: bool) -> Key {
    match vk {
        kb::VK_LCONTROL => Key::LeftCtrl,
        kb::VK_RCONTROL => Key::RightCtrl,
        kb::VK_CONTROL => {
            if extended {
                Key::RightCtrl
            } else {
                Key::LeftCtrl
            }
        }
        kb::VK_LSHIFT => Key::LeftShift,
        kb::VK_RSHIFT => Key::RightShift,
        kb::VK_SHIFT => {
            if scan_code == 0x36 {
                Key::RightShift
            } else {
                Key::LeftShift
            }
        }
        kb::VK_LMENU => Key::LeftAlt,
        kb::VK_RMENU => Key::RightAlt,
        kb::VK_MENU => {
            if extended {
                Key::RightAlt
            } else {
                Key::LeftAlt
            }
        }
        kb::VK_LWIN => Key::LeftWin,
        kb::VK_RWIN => Key::RightWin,
        kb::VK_SPACE => Key::Space,
        kb::VK_RETURN if extended => Key::NumpadEnter,
        kb::VK_RETURN => Key::Enter,
        kb::VK_ESCAPE => Key::Escape,
        kb::VK_TAB => Key::Tab,
        kb::VK_BACK => Key::Backspace,
        kb::VK_CAPITAL => Key::CapsLock,
        kb::VK_NUMLOCK => Key::NumLock,
        kb::VK_SCROLL => Key::ScrollLock,
        kb::VK_LEFT => Key::Left,
        kb::VK_RIGHT => Key::Right,
        kb::VK_UP => Key::Up,
        kb::VK_DOWN => Key::Down,
        kb::VK_HOME => Key::Home,
        kb::VK_END => Key::End,
        kb::VK_PRIOR => Key::PageUp,
        kb::VK_NEXT => Key::PageDown,
        kb::VK_INSERT => Key::Insert,
        kb::VK_DELETE => Key::Delete,
        kb::VK_OEM_1 => Key::Oem1,
        kb::VK_OEM_PLUS => Key::OemPlus,
        kb::VK_OEM_COMMA => Key::OemComma,
        kb::VK_OEM_MINUS => Key::OemMinus,
        kb::VK_OEM_PERIOD => Key::OemPeriod,
        kb::VK_OEM_2 => Key::Oem2,
        kb::VK_OEM_3 => Key::Oem3,
        kb::VK_OEM_4 => Key::Oem4,
        kb::VK_OEM_5 => Key::Oem5,
        kb::VK_OEM_6 => Key::Oem6,
        kb::VK_OEM_7 => Key::Oem7,
        kb::VK_OEM_8 => Key::Oem8,
        kb::VK_OEM_102 => Key::Oem102,
        kb::VK_NUMPAD0 => Key::Numpad0,
        kb::VK_NUMPAD1 => Key::Numpad1,
        kb::VK_NUMPAD2 => Key::Numpad2,
        kb::VK_NUMPAD3 => Key::Numpad3,
        kb::VK_NUMPAD4 => Key::Numpad4,
        kb::VK_NUMPAD5 => Key::Numpad5,
        kb::VK_NUMPAD6 => Key::Numpad6,
        kb::VK_NUMPAD7 => Key::Numpad7,
        kb::VK_NUMPAD8 => Key::Numpad8,
        kb::VK_NUMPAD9 => Key::Numpad9,
        kb::VK_MULTIPLY => Key::NumpadMultiply,
        kb::VK_ADD => Key::NumpadAdd,
        kb::VK_SEPARATOR => Key::NumpadSeparator,
        kb::VK_SUBTRACT => Key::NumpadSubtract,
        kb::VK_DECIMAL => Key::NumpadDecimal,
        kb::VK_DIVIDE => Key::NumpadDivide,
        kb::VK_SNAPSHOT => Key::PrintScreen,
        kb::VK_PAUSE => Key::Pause,
        kb::VK_APPS => Key::Apps,
        kb::VK_BROWSER_BACK => Key::BrowserBack,
        kb::VK_BROWSER_FORWARD => Key::BrowserForward,
        kb::VK_BROWSER_REFRESH => Key::BrowserRefresh,
        kb::VK_BROWSER_STOP => Key::BrowserStop,
        kb::VK_BROWSER_SEARCH => Key::BrowserSearch,
        kb::VK_BROWSER_FAVORITES => Key::BrowserFavorites,
        kb::VK_BROWSER_HOME => Key::BrowserHome,
        kb::VK_VOLUME_MUTE => Key::VolumeMute,
        kb::VK_VOLUME_DOWN => Key::VolumeDown,
        kb::VK_VOLUME_UP => Key::VolumeUp,
        kb::VK_MEDIA_NEXT_TRACK => Key::MediaNextTrack,
        kb::VK_MEDIA_PREV_TRACK => Key::MediaPreviousTrack,
        kb::VK_MEDIA_STOP => Key::MediaStop,
        kb::VK_MEDIA_PLAY_PAUSE => Key::MediaPlayPause,
        0x41 => Key::A,
        0x42 => Key::B,
        0x43 => Key::C,
        0x44 => Key::D,
        0x45 => Key::E,
        0x46 => Key::F,
        0x47 => Key::G,
        0x48 => Key::H,
        0x49 => Key::I,
        0x4A => Key::J,
        0x4B => Key::K,
        0x4C => Key::L,
        0x4D => Key::M,
        0x4E => Key::N,
        0x4F => Key::O,
        0x50 => Key::P,
        0x51 => Key::Q,
        0x52 => Key::R,
        0x53 => Key::S,
        0x54 => Key::T,
        0x55 => Key::U,
        0x56 => Key::V,
        0x57 => Key::W,
        0x58 => Key::X,
        0x59 => Key::Y,
        0x5A => Key::Z,
        0x30 => Key::Digit0,
        0x31 => Key::Digit1,
        0x32 => Key::Digit2,
        0x33 => Key::Digit3,
        0x34 => Key::Digit4,
        0x35 => Key::Digit5,
        0x36 => Key::Digit6,
        0x37 => Key::Digit7,
        0x38 => Key::Digit8,
        0x39 => Key::Digit9,
        kb::VK_F1 => Key::F1,
        kb::VK_F2 => Key::F2,
        kb::VK_F3 => Key::F3,
        kb::VK_F4 => Key::F4,
        kb::VK_F5 => Key::F5,
        kb::VK_F6 => Key::F6,
        kb::VK_F7 => Key::F7,
        kb::VK_F8 => Key::F8,
        kb::VK_F9 => Key::F9,
        kb::VK_F10 => Key::F10,
        kb::VK_F11 => Key::F11,
        kb::VK_F12 => Key::F12,
        kb::VK_F13 => Key::F13,
        kb::VK_F14 => Key::F14,
        kb::VK_F15 => Key::F15,
        kb::VK_F16 => Key::F16,
        kb::VK_F17 => Key::F17,
        kb::VK_F18 => Key::F18,
        kb::VK_F19 => Key::F19,
        kb::VK_F20 => Key::F20,
        kb::VK_F21 => Key::F21,
        kb::VK_F22 => Key::F22,
        kb::VK_F23 => Key::F23,
        kb::VK_F24 => Key::F24,
        other => Key::Unknown(other),
    }
}

/// Convert a named logical key back to its Win32 virtual-key code. Physical
/// identities deliberately return `None` because they must use scan-code
/// output. Unknown observed VKs remain replayable.
pub fn key_to_vk(key: Key) -> Option<u16> {
    Some(match key {
        Key::LeftCtrl => kb::VK_LCONTROL,
        Key::RightCtrl => kb::VK_RCONTROL,
        Key::LeftShift => kb::VK_LSHIFT,
        Key::RightShift => kb::VK_RSHIFT,
        Key::LeftAlt => kb::VK_LMENU,
        Key::RightAlt => kb::VK_RMENU,
        Key::LeftWin => kb::VK_LWIN,
        Key::RightWin => kb::VK_RWIN,
        Key::Space => kb::VK_SPACE,
        Key::Enter | Key::NumpadEnter => kb::VK_RETURN,
        Key::Escape => kb::VK_ESCAPE,
        Key::Tab => kb::VK_TAB,
        Key::Backspace => kb::VK_BACK,
        Key::CapsLock => kb::VK_CAPITAL,
        Key::NumLock => kb::VK_NUMLOCK,
        Key::ScrollLock => kb::VK_SCROLL,
        Key::Left => kb::VK_LEFT,
        Key::Right => kb::VK_RIGHT,
        Key::Up => kb::VK_UP,
        Key::Down => kb::VK_DOWN,
        Key::Home => kb::VK_HOME,
        Key::End => kb::VK_END,
        Key::PageUp => kb::VK_PRIOR,
        Key::PageDown => kb::VK_NEXT,
        Key::Insert => kb::VK_INSERT,
        Key::Delete => kb::VK_DELETE,
        Key::Oem1 => kb::VK_OEM_1,
        Key::OemPlus => kb::VK_OEM_PLUS,
        Key::OemComma => kb::VK_OEM_COMMA,
        Key::OemMinus => kb::VK_OEM_MINUS,
        Key::OemPeriod => kb::VK_OEM_PERIOD,
        Key::Oem2 => kb::VK_OEM_2,
        Key::Oem3 => kb::VK_OEM_3,
        Key::Oem4 => kb::VK_OEM_4,
        Key::Oem5 => kb::VK_OEM_5,
        Key::Oem6 => kb::VK_OEM_6,
        Key::Oem7 => kb::VK_OEM_7,
        Key::Oem8 => kb::VK_OEM_8,
        Key::Oem102 => kb::VK_OEM_102,
        Key::Numpad0 => kb::VK_NUMPAD0,
        Key::Numpad1 => kb::VK_NUMPAD1,
        Key::Numpad2 => kb::VK_NUMPAD2,
        Key::Numpad3 => kb::VK_NUMPAD3,
        Key::Numpad4 => kb::VK_NUMPAD4,
        Key::Numpad5 => kb::VK_NUMPAD5,
        Key::Numpad6 => kb::VK_NUMPAD6,
        Key::Numpad7 => kb::VK_NUMPAD7,
        Key::Numpad8 => kb::VK_NUMPAD8,
        Key::Numpad9 => kb::VK_NUMPAD9,
        Key::NumpadMultiply => kb::VK_MULTIPLY,
        Key::NumpadAdd => kb::VK_ADD,
        Key::NumpadSeparator => kb::VK_SEPARATOR,
        Key::NumpadSubtract => kb::VK_SUBTRACT,
        Key::NumpadDecimal => kb::VK_DECIMAL,
        Key::NumpadDivide => kb::VK_DIVIDE,
        Key::PrintScreen => kb::VK_SNAPSHOT,
        Key::Pause => kb::VK_PAUSE,
        Key::Apps => kb::VK_APPS,
        Key::BrowserBack => kb::VK_BROWSER_BACK,
        Key::BrowserForward => kb::VK_BROWSER_FORWARD,
        Key::BrowserRefresh => kb::VK_BROWSER_REFRESH,
        Key::BrowserStop => kb::VK_BROWSER_STOP,
        Key::BrowserSearch => kb::VK_BROWSER_SEARCH,
        Key::BrowserFavorites => kb::VK_BROWSER_FAVORITES,
        Key::BrowserHome => kb::VK_BROWSER_HOME,
        Key::VolumeMute => kb::VK_VOLUME_MUTE,
        Key::VolumeDown => kb::VK_VOLUME_DOWN,
        Key::VolumeUp => kb::VK_VOLUME_UP,
        Key::MediaNextTrack => kb::VK_MEDIA_NEXT_TRACK,
        Key::MediaPreviousTrack => kb::VK_MEDIA_PREV_TRACK,
        Key::MediaStop => kb::VK_MEDIA_STOP,
        Key::MediaPlayPause => kb::VK_MEDIA_PLAY_PAUSE,
        Key::A => 0x41,
        Key::B => 0x42,
        Key::C => 0x43,
        Key::D => 0x44,
        Key::E => 0x45,
        Key::F => 0x46,
        Key::G => 0x47,
        Key::H => 0x48,
        Key::I => 0x49,
        Key::J => 0x4A,
        Key::K => 0x4B,
        Key::L => 0x4C,
        Key::M => 0x4D,
        Key::N => 0x4E,
        Key::O => 0x4F,
        Key::P => 0x50,
        Key::Q => 0x51,
        Key::R => 0x52,
        Key::S => 0x53,
        Key::T => 0x54,
        Key::U => 0x55,
        Key::V => 0x56,
        Key::W => 0x57,
        Key::X => 0x58,
        Key::Y => 0x59,
        Key::Z => 0x5A,
        Key::Digit0 => 0x30,
        Key::Digit1 => 0x31,
        Key::Digit2 => 0x32,
        Key::Digit3 => 0x33,
        Key::Digit4 => 0x34,
        Key::Digit5 => 0x35,
        Key::Digit6 => 0x36,
        Key::Digit7 => 0x37,
        Key::Digit8 => 0x38,
        Key::Digit9 => 0x39,
        Key::F1 => kb::VK_F1,
        Key::F2 => kb::VK_F2,
        Key::F3 => kb::VK_F3,
        Key::F4 => kb::VK_F4,
        Key::F5 => kb::VK_F5,
        Key::F6 => kb::VK_F6,
        Key::F7 => kb::VK_F7,
        Key::F8 => kb::VK_F8,
        Key::F9 => kb::VK_F9,
        Key::F10 => kb::VK_F10,
        Key::F11 => kb::VK_F11,
        Key::F12 => kb::VK_F12,
        Key::F13 => kb::VK_F13,
        Key::F14 => kb::VK_F14,
        Key::F15 => kb::VK_F15,
        Key::F16 => kb::VK_F16,
        Key::F17 => kb::VK_F17,
        Key::F18 => kb::VK_F18,
        Key::F19 => kb::VK_F19,
        Key::F20 => kb::VK_F20,
        Key::F21 => kb::VK_F21,
        Key::F22 => kb::VK_F22,
        Key::F23 => kb::VK_F23,
        Key::F24 => kb::VK_F24,
        Key::Unknown(vk) => vk,
        Key::Physical { .. } => return None,
    })
}

/// Whether VK-based output for this named key needs the enhanced-key flag.
pub fn key_is_extended(key: Key) -> bool {
    matches!(
        key,
        Key::RightCtrl
            | Key::RightAlt
            | Key::LeftWin
            | Key::RightWin
            | Key::NumpadEnter
            | Key::NumLock
            | Key::Left
            | Key::Right
            | Key::Up
            | Key::Down
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::Insert
            | Key::Delete
            | Key::NumpadDivide
            | Key::PrintScreen
            | Key::Apps
            | Key::BrowserBack
            | Key::BrowserForward
            | Key::BrowserRefresh
            | Key::BrowserStop
            | Key::BrowserSearch
            | Key::BrowserFavorites
            | Key::BrowserHome
            | Key::VolumeMute
            | Key::VolumeDown
            | Key::VolumeUp
            | Key::MediaNextTrack
            | Key::MediaPreviousTrack
            | Key::MediaStop
            | Key::MediaPlayPause
    )
}

/// Map a `WH_MOUSE_LL` message id to a `(button, down)` pair, or `None` for
/// wheel / move messages.
pub fn mouse_wparam(wparam: u32, mouse_data: u32) -> Option<(MouseButton, bool)> {
    match wparam {
        wm::WM_LBUTTONDOWN => Some((MouseButton::Left, true)),
        wm::WM_LBUTTONUP => Some((MouseButton::Left, false)),
        wm::WM_RBUTTONDOWN => Some((MouseButton::Right, true)),
        wm::WM_RBUTTONUP => Some((MouseButton::Right, false)),
        wm::WM_MBUTTONDOWN => Some((MouseButton::Middle, true)),
        wm::WM_MBUTTONUP => Some((MouseButton::Middle, false)),
        wm::WM_XBUTTONDOWN => match high_word(mouse_data) {
            1 => Some((MouseButton::XButton1, true)),
            2 => Some((MouseButton::XButton2, true)),
            _ => None,
        },
        wm::WM_XBUTTONUP => match high_word(mouse_data) {
            1 => Some((MouseButton::XButton1, false)),
            2 => Some((MouseButton::XButton2, false)),
            _ => None,
        },
        _ => None,
    }
}

/// High 16 bits of a 32-bit value.
pub fn high_word(value: u32) -> u16 {
    (value >> 16) as u16
}

/// Signed wheel delta stored in the high word of `mouseData` (120 == one notch).
pub fn wheel_delta(value: u32) -> i32 {
    (high_word(value) as i16) as i32
}

/// Decode a `KBDLLHOOKSTRUCT.flags` value into the `(up, extended, injected)`
/// triple used to build an engine [`inputflow_engine::InputEvent`].
///
/// Low-level keyboard hooks use the `LLKHF_*` bit positions (`LLKHF_UP = 0x80`,
/// `LLKHF_EXTENDED = 0x01`, `LLKHF_INJECTED = 0x10`), not the `KF_*` masks used
/// in window-message `lParam` values. Mixing those families makes releases look
/// like key-downs.
pub fn keyboard_flags(flags: u32) -> (bool, bool, bool) {
    (
        (flags & wm::LLKHF_UP) != 0,
        (flags & wm::LLKHF_EXTENDED) != 0,
        (flags & wm::LLKHF_INJECTED) != 0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_key_round_trips_through_vk_mapping() {
        for &key in Key::NAMED {
            let vk = key_to_vk(key).unwrap_or_else(|| panic!("missing VK for {key}"));
            let scan = if key == Key::RightShift { 0x36 } else { 0 };
            assert_eq!(hook_key(vk, scan, key_is_extended(key)), key, "{key}");
        }
    }

    #[test]
    fn generic_modifiers_and_keypad_enter_use_hook_metadata() {
        assert_eq!(hook_key(kb::VK_CONTROL, 0x1d, false), Key::LeftCtrl);
        assert_eq!(hook_key(kb::VK_CONTROL, 0x1d, true), Key::RightCtrl);
        assert_eq!(hook_key(kb::VK_SHIFT, 0x2a, false), Key::LeftShift);
        assert_eq!(hook_key(kb::VK_SHIFT, 0x36, false), Key::RightShift);
        assert_eq!(hook_key(kb::VK_RETURN, 0x1c, false), Key::Enter);
        assert_eq!(hook_key(kb::VK_RETURN, 0x1c, true), Key::NumpadEnter);
        assert!(key_is_extended(Key::NumLock));
        assert!(key_is_extended(Key::NumpadEnter));
        assert!(!key_is_extended(Key::Enter));
    }

    #[test]
    fn physical_and_unknown_identities_are_not_confused() {
        assert_eq!(hook_key(0xE1, 0, false), Key::Unknown(0xE1));
        assert_eq!(key_to_vk(Key::Unknown(0xE1)), Some(0xE1));
        assert_eq!(
            key_to_vk(Key::Physical {
                scan_code: 0x1e,
                extended: false,
            }),
            None
        );
    }

    #[test]
    fn mouse_wparam_maps_buttons_and_rejects_wheel() {
        assert_eq!(
            mouse_wparam(wm::WM_RBUTTONDOWN, 0),
            Some((MouseButton::Right, true))
        );
        assert_eq!(
            mouse_wparam(wm::WM_RBUTTONUP, 0),
            Some((MouseButton::Right, false))
        );
        assert_eq!(
            mouse_wparam(wm::WM_LBUTTONDOWN, 0),
            Some((MouseButton::Left, true))
        );
        assert_eq!(
            mouse_wparam(wm::WM_XBUTTONDOWN, 1 << 16),
            Some((MouseButton::XButton1, true))
        );
        assert_eq!(mouse_wparam(wm::WM_MOUSEWHEEL, 0), None);
    }

    #[test]
    fn keyboard_flags_decodes_low_level_hook_bits() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            LLKHF_EXTENDED, LLKHF_INJECTED, LLKHF_UP,
        };

        assert_eq!(keyboard_flags(0), (false, false, false));
        assert_eq!(keyboard_flags(LLKHF_UP), (true, false, false));
        assert_eq!(keyboard_flags(LLKHF_EXTENDED), (false, true, false));
        assert_eq!(keyboard_flags(LLKHF_INJECTED), (false, false, true));
        assert_eq!(
            keyboard_flags(LLKHF_UP | LLKHF_EXTENDED | LLKHF_INJECTED),
            (true, true, true)
        );
    }
}
