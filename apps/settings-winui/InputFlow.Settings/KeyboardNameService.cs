using System.Runtime.InteropServices;
using System.Text;
using InputFlow.Settings.Core;

namespace InputFlow_Settings;

internal static class KeyboardNameService
{
    private const uint MapvkVkToVscEx = 4;

    public static string DisplayName(KeyIdentity identity)
    {
        uint scan;
        bool extended;
        if (identity.Mode == KeyMatchMode.Physical)
        {
            scan = identity.ScanCode;
            extended = identity.Extended;
        }
        else
        {
            uint? virtualKey = VirtualKey(identity.LogicalKey!);
            if (virtualKey is null)
            {
                return identity.StableName;
            }

            if (identity.LogicalKey == "NumpadEnter")
            {
                scan = 0x1C;
                extended = true;
            }
            else
            {
                uint mapped = MapVirtualKeyEx(virtualKey.Value, MapvkVkToVscEx, GetKeyboardLayout(0));
                if (mapped == 0)
                {
                    return identity.StableName;
                }

                scan = mapped & 0xFF;
                extended = (mapped & 0xFF00) != 0 || IsExtendedLogical(identity.LogicalKey!);
            }
        }

        int lParam = checked((int)(scan << 16)) | (extended ? 1 << 24 : 0);
        var name = new StringBuilder(128);
        return GetKeyNameText(lParam, name, name.Capacity) > 0
            ? name.ToString()
            : identity.StableName;
    }

    private static uint? VirtualKey(string key)
    {
        if (key.Length == 1 && key[0] is >= 'A' and <= 'Z') return key[0];
        if (key.StartsWith("Digit", StringComparison.Ordinal) && int.TryParse(key[5..], out int digit))
            return (uint)('0' + digit);
        if (key.StartsWith('F') && int.TryParse(key[1..], out int function) && function is >= 1 and <= 24)
            return (uint)(0x70 + function - 1);
        if (key.StartsWith("Numpad", StringComparison.Ordinal) &&
            int.TryParse(key[6..], out int numpadDigit))
            return (uint)(0x60 + numpadDigit);

        return key switch
        {
            "LeftCtrl" => 0xA2, "RightCtrl" => 0xA3,
            "LeftShift" => 0xA0, "RightShift" => 0xA1,
            "LeftAlt" => 0xA4, "RightAlt" => 0xA5,
            "LeftWin" => 0x5B, "RightWin" => 0x5C,
            "Space" => 0x20, "Enter" or "NumpadEnter" => 0x0D, "Escape" => 0x1B,
            "Tab" => 0x09, "Backspace" => 0x08, "CapsLock" => 0x14,
            "NumLock" => 0x90, "ScrollLock" => 0x91,
            "Left" => 0x25, "Up" => 0x26, "Right" => 0x27, "Down" => 0x28,
            "Home" => 0x24, "End" => 0x23, "PageUp" => 0x21, "PageDown" => 0x22,
            "Insert" => 0x2D, "Delete" => 0x2E,
            "Oem1" => 0xBA, "OemPlus" => 0xBB, "OemComma" => 0xBC,
            "OemMinus" => 0xBD, "OemPeriod" => 0xBE, "Oem2" => 0xBF,
            "Oem3" => 0xC0, "Oem4" => 0xDB, "Oem5" => 0xDC, "Oem6" => 0xDD,
            "Oem7" => 0xDE, "Oem8" => 0xDF, "Oem102" => 0xE2,
            "NumpadMultiply" => 0x6A, "NumpadAdd" => 0x6B, "NumpadSeparator" => 0x6C,
            "NumpadSubtract" => 0x6D, "NumpadDecimal" => 0x6E, "NumpadDivide" => 0x6F,
            "PrintScreen" => 0x2C, "Pause" => 0x13, "Apps" => 0x5D,
            "BrowserBack" => 0xA6, "BrowserForward" => 0xA7, "BrowserRefresh" => 0xA8,
            "BrowserStop" => 0xA9, "BrowserSearch" => 0xAA, "BrowserFavorites" => 0xAB,
            "BrowserHome" => 0xAC, "VolumeMute" => 0xAD, "VolumeDown" => 0xAE,
            "VolumeUp" => 0xAF, "MediaNextTrack" => 0xB0, "MediaPreviousTrack" => 0xB1,
            "MediaStop" => 0xB2, "MediaPlayPause" => 0xB3,
            _ => null,
        };
    }

    private static bool IsExtendedLogical(string key) => key is
        "RightCtrl" or "RightAlt" or "LeftWin" or "RightWin" or "NumpadEnter" or
        "Insert" or "Delete" or "Home" or "End" or "PageUp" or "PageDown" or
        "Left" or "Right" or "Up" or "Down" or "NumpadDivide" or "PrintScreen" or
        "Apps" or "BrowserBack" or "BrowserForward" or "BrowserRefresh" or "BrowserStop" or
        "BrowserSearch" or "BrowserFavorites" or "BrowserHome" or "VolumeMute" or
        "VolumeDown" or "VolumeUp" or "MediaNextTrack" or "MediaPreviousTrack" or
        "MediaStop" or "MediaPlayPause";

    [DllImport("user32.dll", EntryPoint = "GetKeyboardLayout")]
    private static extern nint GetKeyboardLayout(uint threadId);

    [DllImport("user32.dll", EntryPoint = "MapVirtualKeyExW")]
    private static extern uint MapVirtualKeyEx(uint code, uint mapType, nint keyboardLayout);

    [DllImport("user32.dll", EntryPoint = "GetKeyNameTextW", CharSet = CharSet.Unicode)]
    private static extern int GetKeyNameText(int lParam, StringBuilder buffer, int size);
}
