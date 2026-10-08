using System.Buffers;
using System.Text;
using System.Text.Json;

namespace InputFlow.Settings.Core;

public enum KeyMatchMode
{
    Logical,
    Physical,
}

public enum MouseDirection
{
    Left,
    Right,
    Up,
    Down,
}

public sealed record KeyIdentity
{
    private KeyIdentity(KeyMatchMode mode, string? logicalKey, ushort scanCode, bool extended)
    {
        Mode = mode;
        LogicalKey = logicalKey;
        ScanCode = scanCode;
        Extended = extended;
    }

    public KeyMatchMode Mode { get; }

    public string? LogicalKey { get; }

    public ushort ScanCode { get; }

    public bool Extended { get; }

    public static KeyIdentity Logical(string key)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(key);
        return new KeyIdentity(KeyMatchMode.Logical, key, 0, false);
    }

    public static KeyIdentity Physical(ushort scanCode, bool extended)
    {
        if (scanCode == 0)
        {
            throw new ArgumentOutOfRangeException(nameof(scanCode), "Physical scan code must be nonzero");
        }

        return new KeyIdentity(KeyMatchMode.Physical, null, scanCode, extended);
    }

    public string StableName => Mode == KeyMatchMode.Logical
        ? LogicalKey!
        : $"Scan 0x{ScanCode:X2}{(Extended ? " (extended)" : string.Empty)}";
}

public abstract record RuleTrigger
{
    public abstract string Type { get; }

    public abstract KeyIdentity FirstKey { get; }

    public abstract RuleTrigger DeepCopy();
}

public sealed record KeyChordTrigger(KeyIdentity First, KeyIdentity Second) : RuleTrigger
{
    public override string Type => "key_chord";

    public override KeyIdentity FirstKey => First;

    public override RuleTrigger DeepCopy() => new KeyChordTrigger(First, Second);
}

public sealed record KeyMouseButtonTrigger(KeyIdentity Key, string Button) : RuleTrigger
{
    public override string Type => "key_mouse_button";

    public override KeyIdentity FirstKey => Key;

    public override RuleTrigger DeepCopy() => new KeyMouseButtonTrigger(Key, Button);
}

public sealed record HoldTrigger(KeyIdentity Key, ulong TimeoutMilliseconds) : RuleTrigger
{
    public override string Type => "hold";

    public override KeyIdentity FirstKey => Key;

    public override RuleTrigger DeepCopy() => new HoldTrigger(Key, TimeoutMilliseconds);
}

public sealed record HoldMouseButtonTrigger(
    KeyIdentity Key,
    ulong TimeoutMilliseconds,
    string Button) : RuleTrigger
{
    public override string Type => "hold_mouse_button";

    public override KeyIdentity FirstKey => Key;

    public override RuleTrigger DeepCopy() => new HoldMouseButtonTrigger(Key, TimeoutMilliseconds, Button);
}

public sealed record MouseDirectionTrigger(
    KeyIdentity Key,
    MouseDirection Direction,
    uint MinimumDistancePixels,
    ulong MaximumDurationMilliseconds,
    uint OffAxisTolerancePixels) : RuleTrigger
{
    public override string Type => "mouse_direction";

    public override KeyIdentity FirstKey => Key;

    public override RuleTrigger DeepCopy() => new MouseDirectionTrigger(
        Key,
        Direction,
        MinimumDistancePixels,
        MaximumDurationMilliseconds,
        OffAxisTolerancePixels);
}

public sealed record KeyChordAction
{
    public KeyChordAction(IEnumerable<KeyIdentity> keys)
    {
        Keys = keys.ToList();
    }

    public List<KeyIdentity> Keys { get; }

    public KeyChordAction DeepCopy() => new(Keys);
}

public sealed record RuleDocument
{
    public required string Id { get; init; }

    public required bool Enabled { get; init; }

    public required RuleTrigger Trigger { get; init; }

    public required KeyChordAction Action { get; init; }

    public RuleDocument DeepCopy() => new()
    {
        Id = Id,
        Enabled = Enabled,
        Trigger = Trigger.DeepCopy(),
        Action = Action.DeepCopy(),
    };
}

public sealed record ConfigDocument
{
    public const uint CurrentSchemaVersion = 4;

    public uint SchemaVersion { get; init; } = CurrentSchemaVersion;

    public required KeyIdentity EmergencyBypassKey { get; init; }

    public required List<RuleDocument> Rules { get; init; }

    public ConfigDocument DeepCopy() => new()
    {
        SchemaVersion = SchemaVersion,
        EmergencyBypassKey = EmergencyBypassKey,
        Rules = Rules.Select(rule => rule.DeepCopy()).ToList(),
    };

    public int EnabledRuleCount => Rules.Count(rule => rule.Enabled);
}

public static class ConfigCodec
{
    public static ConfigDocument Parse(JsonElement root)
    {
        uint sourceVersion = root.TryGetProperty("schema_version", out JsonElement version)
            ? version.GetUInt32()
            : 1;
        if (sourceVersion is < 1 or > ConfigDocument.CurrentSchemaVersion)
        {
            throw new FormatException($"Unsupported schema_version {sourceVersion}");
        }

        KeyIdentity emergency = sourceVersion == 1
            ? KeyIdentity.Logical(root.GetProperty("emergency_bypass_key").GetString()
                ?? throw new FormatException("emergency_bypass_key must be a string"))
            : ParseKey(root.GetProperty("emergency_bypass_key"));

        var rules = new List<RuleDocument>();
        foreach (JsonElement rule in root.GetProperty("rules").EnumerateArray())
        {
            rules.Add(new RuleDocument
            {
                Id = rule.GetProperty("id").GetString()
                    ?? throw new FormatException("rule id must be a string"),
                Enabled = sourceVersion < 3 || rule.GetProperty("enabled").GetBoolean(),
                Trigger = ParseTrigger(rule.GetProperty("trigger"), sourceVersion),
                Action = ParseAction(rule.GetProperty("action"), sourceVersion),
            });
        }

        return new ConfigDocument
        {
            SchemaVersion = ConfigDocument.CurrentSchemaVersion,
            EmergencyBypassKey = emergency,
            Rules = rules,
        };
    }

    public static JsonElement ToJsonElement(ConfigDocument config)
    {
        using JsonDocument document = JsonDocument.Parse(ToUtf8(config, indented: false));
        return document.RootElement.Clone();
    }

    public static string ToJson(ConfigDocument config, bool indented = false)
    {
        return Encoding.UTF8.GetString(ToUtf8(config, indented));
    }

    public static bool DeepEquals(ConfigDocument left, ConfigDocument right)
    {
        JsonElement leftJson = ToJsonElement(left);
        JsonElement rightJson = ToJsonElement(right);
        return JsonElement.DeepEquals(leftJson, rightJson);
    }

    private static byte[] ToUtf8(ConfigDocument config, bool indented)
    {
        ArgumentNullException.ThrowIfNull(config);

        var buffer = new ArrayBufferWriter<byte>();
        using var writer = new Utf8JsonWriter(buffer, new JsonWriterOptions { Indented = indented });
        writer.WriteStartObject();
        writer.WriteNumber("schema_version", ConfigDocument.CurrentSchemaVersion);
        writer.WritePropertyName("emergency_bypass_key");
        WriteKey(writer, config.EmergencyBypassKey);
        writer.WriteStartArray("rules");
        foreach (RuleDocument rule in config.Rules)
        {
            writer.WriteStartObject();
            writer.WriteString("id", rule.Id);
            writer.WriteBoolean("enabled", rule.Enabled);
            writer.WritePropertyName("trigger");
            WriteTrigger(writer, rule.Trigger);
            writer.WritePropertyName("action");
            WriteAction(writer, rule.Action);
            writer.WriteEndObject();
        }

        writer.WriteEndArray();
        writer.WriteEndObject();
        writer.Flush();
        return buffer.WrittenSpan.ToArray();
    }

    private static RuleTrigger ParseTrigger(JsonElement trigger, uint sourceVersion)
    {
        string type = trigger.GetProperty("type").GetString()
            ?? throw new FormatException("trigger type must be a string");
        KeyIdentity Key(string property) => sourceVersion == 1
            ? KeyIdentity.Logical(trigger.GetProperty(property).GetString()
                ?? throw new FormatException($"{property} must be a key string"))
            : ParseKey(trigger.GetProperty(property));

        return type switch
        {
            "key_chord" => new KeyChordTrigger(Key("first"), Key("second")),
            "key_mouse_button" => new KeyMouseButtonTrigger(
                Key("key"),
                RequiredString(trigger, "button")),
            "hold" => new HoldTrigger(Key("key"), trigger.GetProperty("timeout_ms").GetUInt64()),
            "hold_mouse_button" => new HoldMouseButtonTrigger(
                Key("key"),
                trigger.GetProperty("timeout_ms").GetUInt64(),
                RequiredString(trigger, "button")),
            "mouse_direction" when sourceVersion >= 4 => new MouseDirectionTrigger(
                Key("key"),
                ParseDirection(RequiredString(trigger, "direction")),
                trigger.GetProperty("min_distance_px").GetUInt32(),
                trigger.GetProperty("max_duration_ms").GetUInt64(),
                trigger.GetProperty("off_axis_tolerance_px").GetUInt32()),
            _ => throw new FormatException($"Unsupported trigger type `{type}`"),
        };
    }

    private static KeyChordAction ParseAction(JsonElement action, uint sourceVersion)
    {
        string type = RequiredString(action, "type");
        if (type != "key_chord")
        {
            throw new FormatException($"Unsupported action type `{type}`");
        }

        IEnumerable<KeyIdentity> keys = action.GetProperty("keys").EnumerateArray().Select(key =>
            sourceVersion == 1
                ? KeyIdentity.Logical(key.GetString()
                    ?? throw new FormatException("action key must be a string"))
                : ParseKey(key));
        return new KeyChordAction(keys);
    }

    private static KeyIdentity ParseKey(JsonElement key)
    {
        string mode = RequiredString(key, "match");
        return mode switch
        {
            "logical" => KeyIdentity.Logical(RequiredString(key, "key")),
            "physical" => KeyIdentity.Physical(
                key.GetProperty("scan_code").GetUInt16(),
                key.GetProperty("extended").GetBoolean()),
            _ => throw new FormatException($"Unsupported key match mode `{mode}`"),
        };
    }

    private static void WriteTrigger(Utf8JsonWriter writer, RuleTrigger trigger)
    {
        writer.WriteStartObject();
        writer.WriteString("type", trigger.Type);
        switch (trigger)
        {
            case KeyChordTrigger chord:
                writer.WritePropertyName("first");
                WriteKey(writer, chord.First);
                writer.WritePropertyName("second");
                WriteKey(writer, chord.Second);
                break;
            case KeyMouseButtonTrigger mouse:
                writer.WritePropertyName("key");
                WriteKey(writer, mouse.Key);
                writer.WriteString("button", mouse.Button);
                break;
            case HoldTrigger hold:
                writer.WritePropertyName("key");
                WriteKey(writer, hold.Key);
                writer.WriteNumber("timeout_ms", hold.TimeoutMilliseconds);
                break;
            case HoldMouseButtonTrigger holdMouse:
                writer.WritePropertyName("key");
                WriteKey(writer, holdMouse.Key);
                writer.WriteNumber("timeout_ms", holdMouse.TimeoutMilliseconds);
                writer.WriteString("button", holdMouse.Button);
                break;
            case MouseDirectionTrigger direction:
                writer.WritePropertyName("key");
                WriteKey(writer, direction.Key);
                writer.WriteString("direction", direction.Direction.ToString().ToLowerInvariant());
                writer.WriteNumber("min_distance_px", direction.MinimumDistancePixels);
                writer.WriteNumber("max_duration_ms", direction.MaximumDurationMilliseconds);
                writer.WriteNumber("off_axis_tolerance_px", direction.OffAxisTolerancePixels);
                break;
            default:
                throw new InvalidOperationException($"Unknown trigger type {trigger.GetType().Name}");
        }

        writer.WriteEndObject();
    }

    private static void WriteAction(Utf8JsonWriter writer, KeyChordAction action)
    {
        writer.WriteStartObject();
        writer.WriteString("type", "key_chord");
        writer.WriteStartArray("keys");
        foreach (KeyIdentity key in action.Keys)
        {
            WriteKey(writer, key);
        }

        writer.WriteEndArray();
        writer.WriteEndObject();
    }

    private static void WriteKey(Utf8JsonWriter writer, KeyIdentity key)
    {
        writer.WriteStartObject();
        switch (key.Mode)
        {
            case KeyMatchMode.Logical:
                writer.WriteString("match", "logical");
                writer.WriteString("key", key.LogicalKey);
                break;
            case KeyMatchMode.Physical:
                writer.WriteString("match", "physical");
                writer.WriteNumber("scan_code", key.ScanCode);
                writer.WriteBoolean("extended", key.Extended);
                break;
            default:
                throw new InvalidOperationException($"Unknown key mode {key.Mode}");
        }

        writer.WriteEndObject();
    }

    private static string RequiredString(JsonElement element, string property)
    {
        return element.GetProperty(property).GetString()
            ?? throw new FormatException($"{property} must be a string");
    }

    private static MouseDirection ParseDirection(string value) => value switch
    {
        "left" => MouseDirection.Left,
        "right" => MouseDirection.Right,
        "up" => MouseDirection.Up,
        "down" => MouseDirection.Down,
        _ => throw new FormatException($"Unsupported mouse direction `{value}`"),
    };
}

public static class InputCatalog
{
    public static IReadOnlyList<string> LogicalKeys { get; } = BuildLogicalKeys();

    public static IReadOnlyList<string> MouseButtons { get; } =
        ["Left", "Right", "Middle", "XButton1", "XButton2"];

    public static IReadOnlyList<MouseDirection> MouseDirections { get; } =
        [MouseDirection.Left, MouseDirection.Right, MouseDirection.Up, MouseDirection.Down];

    public static string GroupName(KeyIdentity key)
    {
        if (key.Mode == KeyMatchMode.Physical)
        {
            return key.StableName;
        }

        string logical = key.LogicalKey!;
        if (logical is "LeftCtrl" or "RightCtrl") return "Ctrl";
        if (logical is "LeftShift" or "RightShift") return "Shift";
        if (logical is "LeftAlt" or "RightAlt") return "Alt";
        if (logical.StartsWith("Oem", StringComparison.Ordinal)) return "符号键";
        if (logical.StartsWith("Digit", StringComparison.Ordinal)) return "数字";
        if (logical.StartsWith("Numpad", StringComparison.Ordinal)) return "小键盘";
        if (logical.Length == 1 && char.IsAsciiLetter(logical[0])) return "字母";
        return logical switch
        {
            "CapsLock" => "Caps Lock",
            "Space" => "空格",
            _ => logical,
        };
    }

    private static IReadOnlyList<string> BuildLogicalKeys()
    {
        var keys = new List<string>
        {
            "LeftCtrl", "RightCtrl", "LeftShift", "RightShift", "LeftAlt", "RightAlt",
            "LeftWin", "RightWin",
        };
        keys.AddRange(Enumerable.Range('A', 26).Select(value => ((char)value).ToString()));
        keys.AddRange(Enumerable.Range(0, 10).Select(value => $"Digit{value}"));
        keys.AddRange(Enumerable.Range(1, 24).Select(value => $"F{value}"));
        keys.AddRange([
            "Space", "Enter", "NumpadEnter", "Escape", "Tab", "Backspace", "CapsLock",
            "NumLock", "ScrollLock", "Left", "Right", "Up", "Down", "Home", "End",
            "PageUp", "PageDown", "Insert", "Delete", "Oem1", "OemPlus", "OemComma",
            "OemMinus", "OemPeriod", "Oem2", "Oem3", "Oem4", "Oem5", "Oem6", "Oem7",
            "Oem8", "Oem102",
        ]);
        keys.AddRange(Enumerable.Range(0, 10).Select(value => $"Numpad{value}"));
        keys.AddRange([
            "NumpadMultiply", "NumpadAdd", "NumpadSeparator", "NumpadSubtract",
            "NumpadDecimal", "NumpadDivide", "PrintScreen", "Pause", "Apps", "BrowserBack",
            "BrowserForward", "BrowserRefresh", "BrowserStop", "BrowserSearch", "BrowserFavorites",
            "BrowserHome", "VolumeMute", "VolumeDown", "VolumeUp", "MediaNextTrack",
            "MediaPreviousTrack", "MediaStop", "MediaPlayPause",
        ]);
        return keys;
    }
}
