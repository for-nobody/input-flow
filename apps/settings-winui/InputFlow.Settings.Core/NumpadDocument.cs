using System.Text.Json;

namespace InputFlow.Settings.Core;

public enum NumpadGuide { Always, TwoSeconds, UntilFirstInput, Never }

public sealed record NumpadDocument(bool Enabled, RuleTrigger Start, RuleTrigger Stop, NumpadGuide Guide)
{
    public VirtualPanelDocument? Panel { get; init; }

    public static NumpadDocument CreateDefault() => new(false,
        new DoubleTapTrigger(KeyIdentity.Logical("F9")),
        new DoubleTapTrigger(KeyIdentity.Logical("F10")),
        NumpadGuide.UntilFirstInput);

    internal static NumpadDocument Parse(JsonElement value, bool isPanel = false)
    {
        var defaults = CreateDefault();
        return new(
            value.TryGetProperty("enabled", out JsonElement enabled) && enabled.GetBoolean(),
            value.TryGetProperty("start", out JsonElement start) ? ConfigCodec.ParseTrigger(start, 7) : defaults.Start,
            value.TryGetProperty("stop", out JsonElement stop) ? ConfigCodec.ParseTrigger(stop, 7) : defaults.Stop,
            value.TryGetProperty("guide", out JsonElement guide) ? guide.GetString() switch
            {
                "always" => NumpadGuide.Always,
                "two_seconds" => NumpadGuide.TwoSeconds,
                "until_first_input" => NumpadGuide.UntilFirstInput,
                "never" => NumpadGuide.Never,
                _ => throw new FormatException("Unknown numpad guide policy"),
            } : defaults.Guide)
        {
            Panel = isPanel ? VirtualPanelDocument.Parse(value) : null,
        };
    }

    internal void Write(Utf8JsonWriter writer, bool isPanel)
    {
        writer.WriteStartObject();
        writer.WriteBoolean("enabled", Enabled);
        writer.WritePropertyName("start");
        ConfigCodec.WriteTrigger(writer, Start);
        writer.WritePropertyName("stop");
        ConfigCodec.WriteTrigger(writer, Stop);
        writer.WriteString("guide", Guide switch
        {
            NumpadGuide.Always => "always",
            NumpadGuide.TwoSeconds => "two_seconds",
            NumpadGuide.UntilFirstInput => "until_first_input",
            NumpadGuide.Never => "never",
            _ => throw new ArgumentOutOfRangeException(nameof(Guide)),
        });
        if (isPanel)
        {
            (Panel ?? VirtualPanelDocument.CreateDefault()).Write(writer);
        }
        writer.WriteEndObject();
    }
}
