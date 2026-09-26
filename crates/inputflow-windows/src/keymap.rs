//! Mapping between Win32 virtual-key codes / mouse messages and the
//! platform-independent [`inputflow_engine`] types.
//!
//! Pure logic with no `unsafe`; unit-testable without installing a hook. The
//! [`Key::Unknown`] fallback preserves the raw platform key code for keys that
//! are not modelled yet, so no key identity is lost.

use inputflow_engine::{Key, MouseButton};
use windows_sys::Win32::UI::Input::KeyboardAndMouse as kb;
use windows_sys::Win32::UI::WindowsAndMessaging as wm;

/// Convert a Win32 virtual-key code to a platform-independent [`Key`].
pub fn vk_to_key(vk: u16) -> Key {
    match vk {
        kb::VK_LCONTROL | kb::VK_CONTROL => Key::LeftCtrl,
        kb::VK_RCONTROL => Key::RightCtrl,
        kb::VK_LSHIFT | kb::VK_SHIFT => Key::LeftShift,
        kb::VK_RSHIFT => Key::RightShift,
        kb::VK_LMENU | kb::VK_MENU => Key::LeftAlt,
        kb::VK_RMENU => Key::RightAlt,
        kb::VK_LWIN => Key::LeftWin,
        kb::VK_RWIN => Key::RightWin,
        kb::VK_SPACE => Key::Space,
        kb::VK_RETURN => Key::Enter,
        kb::VK_ESCAPE => Key::Escape,
        kb::VK_TAB => Key::Tab,
        kb::VK_BACK => Key::Backspace,
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

/// Convert a platform-independent [`Key`] back to its Win32 virtual-key code,
/// used when synthesizing output. [`Key::Unknown`] round-trips its raw code.
pub fn key_to_vk(key: Key) -> u16 {
    match key {
        Key::LeftCtrl => kb::VK_LCONTROL,
        Key::RightCtrl => kb::VK_RCONTROL,
        Key::LeftShift => kb::VK_LSHIFT,
        Key::RightShift => kb::VK_RSHIFT,
        Key::LeftAlt => kb::VK_LMENU,
        Key::RightAlt => kb::VK_RMENU,
        Key::LeftWin => kb::VK_LWIN,
        Key::RightWin => kb::VK_RWIN,
        Key::Space => kb::VK_SPACE,
        Key::Enter => kb::VK_RETURN,
        Key::Escape => kb::VK_ESCAPE,
        Key::Tab => kb::VK_TAB,
        Key::Backspace => kb::VK_BACK,
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
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_key_round_trips_for_modelled_keys() {
        let keys = [
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
            Key::F24,
            Key::Space,
            Key::Enter,
            Key::Escape,
            Key::Tab,
            Key::Backspace,
        ];
        for key in keys {
            assert_eq!(vk_to_key(key_to_vk(key)), key);
        }
    }

    #[test]
    fn unknown_key_round_trips_raw_code() {
        assert_eq!(vk_to_key(0x1234), Key::Unknown(0x1234));
        assert_eq!(key_to_vk(Key::Unknown(0x1234)), 0x1234);
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
}
