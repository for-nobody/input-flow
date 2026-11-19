using System.Text.Json;

namespace InputFlow.Settings.Core;

public enum PointerStyle { Ring, Dot, Ink }

public sealed record PointerAppearance(
    PointerStyle Type, string Color, uint Diameter, uint Thickness, uint Width, uint? RetainSeconds);

public sealed record PresentationPreset(string Id, string Name, RuleTrigger? Binding, PointerAppearance Style);

public sealed record PresentationDocument(
    bool Enabled, RuleTrigger Start, RuleTrigger Stop, RuleTrigger? TemporaryMouse, RuleTrigger? Clear,
    IReadOnlyList<PresentationPreset> Presets, string DefaultPreset, string? Display, bool Absolute)
{
    public static PresentationDocument CreateDefault()
    {
        RuleTrigger Alt(string digit) => new KeyChordTrigger(KeyIdentity.Logical("LeftAlt"), KeyIdentity.Logical(digit));
        return new(false, new DoubleTapTrigger(KeyIdentity.Logical("LeftCtrl")),
            new DoubleTapTrigger(KeyIdentity.Logical("RightCtrl")), Alt("Digit4"), Alt("Digit5"),
            [
                new("red-ring", "Red ring", Alt("Digit1"), new(PointerStyle.Ring, "#FF3333", 28, 3, 5, null)),
                new("yellow-dot", "Yellow dot", Alt("Digit2"), new(PointerStyle.Dot, "#FFE500", 18, 3, 5, null)),
                new("blue-ink", "Blue ink", Alt("Digit3"), new(PointerStyle.Ink, "#2684FF", 28, 3, 5, null)),
            ], "red-ring", null, false);
    }

    public PresentationDocument DeepCopy() => this with { Presets = Presets.ToArray() };
}

internal static class PresentationCodec
{
    public static PresentationDocument Parse(JsonElement value)
    {
        var defaults = PresentationDocument.CreateDefault();
        RuleTrigger? Binding(JsonElement source, string name, RuleTrigger? fallback = null) =>
            source.TryGetProperty(name, out JsonElement binding)
                ? binding.ValueKind == JsonValueKind.Null ? null : ConfigCodec.ParseTrigger(binding, 6)
                : fallback;
        IReadOnlyList<PresentationPreset> presets = defaults.Presets;
        if (value.TryGetProperty("presets", out JsonElement list))
        {
            presets = list.EnumerateArray().Select(item =>
                new PresentationPreset(String(item, "id"), String(item, "name"),
                    Binding(item, "binding"), ReadStyle(item.GetProperty("style")))).ToArray();
        }
        return new(
            value.TryGetProperty("enabled", out JsonElement enabled) && enabled.GetBoolean(),
            Binding(value, "start", defaults.Start) ?? throw new FormatException("Missing presentation.start"),
            Binding(value, "stop", defaults.Stop) ?? throw new FormatException("Missing presentation.stop"),
            Binding(value, "temporary_mouse", defaults.TemporaryMouse),
            Binding(value, "clear", defaults.Clear),
            presets,
            value.TryGetProperty("default_preset", out JsonElement active) ? active.GetString() ?? "" : defaults.DefaultPreset,
            value.TryGetProperty("display", out JsonElement display) ? display.GetString() : null,
            value.TryGetProperty("movement", out JsonElement movement) && movement.GetString() == "absolute");
    }

    private static PointerAppearance ReadStyle(JsonElement value)
    {
        string color = String(value, "color");
        return String(value, "type") switch
        {
            "ring" => new(PointerStyle.Ring, color, value.GetProperty("diameter").GetUInt32(), value.GetProperty("thickness").GetUInt32(), 5, null),
            "dot" => new(PointerStyle.Dot, color, value.GetProperty("diameter").GetUInt32(), 3, 5, null),
            "ink" => new(PointerStyle.Ink, color, 28, 3, value.GetProperty("width").GetUInt32(),
                value.TryGetProperty("retain_seconds", out JsonElement seconds) && seconds.ValueKind != JsonValueKind.Null ? seconds.GetUInt32() : null),
            var type => throw new FormatException($"Unknown presentation style {type}"),
        };
    }

    public static void Write(Utf8JsonWriter writer, PresentationDocument value)
    {
        writer.WriteStartObject();
        writer.WriteBoolean("enabled", value.Enabled);
        WriteBinding(writer, "start", value.Start);
        WriteBinding(writer, "stop", value.Stop);
        WriteBinding(writer, "temporary_mouse", value.TemporaryMouse);
        WriteBinding(writer, "clear", value.Clear);
        writer.WriteString("default_preset", value.DefaultPreset);
        writer.WriteString("display", value.Display);
        writer.WriteString("movement", value.Absolute ? "absolute" : "relative");
        writer.WriteStartArray("presets");
        foreach (PresentationPreset preset in value.Presets)
        {
            writer.WriteStartObject();
            writer.WriteString("id", preset.Id);
            writer.WriteString("name", preset.Name);
            WriteBinding(writer, "binding", preset.Binding);
            writer.WriteStartObject("style");
            var style = preset.Style;
            writer.WriteString("type", style.Type.ToString().ToLowerInvariant());
            writer.WriteString("color", style.Color);
            if (style.Type == PointerStyle.Ink)
            {
                writer.WriteNumber("width", style.Width);
                if (style.RetainSeconds is uint seconds) writer.WriteNumber("retain_seconds", seconds);
                else writer.WriteNull("retain_seconds");
            }
            else
            {
                writer.WriteNumber("diameter", style.Diameter);
                if (style.Type == PointerStyle.Ring) writer.WriteNumber("thickness", style.Thickness);
            }
            writer.WriteEndObject();
            writer.WriteEndObject();
        }
        writer.WriteEndArray();
        writer.WriteEndObject();
    }

    private static void WriteBinding(Utf8JsonWriter writer, string name, RuleTrigger? value)
    {
        writer.WritePropertyName(name);
        if (value is null) writer.WriteNullValue();
        else ConfigCodec.WriteTrigger(writer, value);
    }

    private static string String(JsonElement value, string name) =>
        value.GetProperty(name).GetString() ?? throw new FormatException($"Missing {name}");
}
