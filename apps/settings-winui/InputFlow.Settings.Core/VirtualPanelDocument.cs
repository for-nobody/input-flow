using System.Text.Json;

namespace InputFlow.Settings.Core;

public readonly record struct PanelPoint(double X, double Y)
{
    public static PanelPoint operator +(PanelPoint a, PanelPoint b) => new(a.X + b.X, a.Y + b.Y);
    public static PanelPoint operator -(PanelPoint a, PanelPoint b) => new(a.X - b.X, a.Y - b.Y);
    public static PanelPoint operator *(PanelPoint a, double scale) => new(a.X * scale, a.Y * scale);
}

public enum PanelOperator { Add, Subtract, Multiply, Divide }

public abstract record PanelAction;
public sealed record PanelDigit(int Digit) : PanelAction;
public sealed record PanelArithmetic(PanelOperator Operator) : PanelAction;
public sealed record PanelKeys(IReadOnlyList<KeyIdentity> Keys) : PanelAction;

public sealed record PanelRegion(string Id, string Name, PanelAction? Action, IReadOnlyList<IReadOnlyList<PanelPoint>> Faces);
public sealed record PanelGeometry(PanelPoint Gap, IReadOnlyList<PanelRegion> Regions);
public sealed record PanelLayout(string Id, string Name, RuleTrigger? Select, PanelGeometry Geometry)
{
    public static PanelLayout Blank(string name) => new(NewId(), name, null, new(new(.005, .005), []));

    public PanelLayout Copy(string name) => new(NewId(), name, null, Geometry with
    {
        Regions = Geometry.Regions.Select(region => region with { Id = NewId() }).ToArray(),
    });

    public static string NewId() => Guid.NewGuid().ToString("N");
}

public sealed record VirtualPanelDocument(string? DefaultLayout, IReadOnlyList<PanelLayout> Layouts)
{
    public VirtualPanelDocument Add(PanelLayout layout) => this with
    {
        Layouts = [.. Layouts, layout],
        DefaultLayout = DefaultLayout ?? layout.Id,
    };

    public VirtualPanelDocument Replace(PanelLayout layout) => this with
    {
        Layouts = Layouts.Select(current => current.Id == layout.Id ? layout : current).ToArray(),
    };

    public VirtualPanelDocument Remove(string id)
    {
        PanelLayout[] remaining = Layouts.Where(layout => layout.Id != id).ToArray();
        return new(DefaultLayout == id ? remaining.FirstOrDefault()?.Id : DefaultLayout, remaining);
    }

    public static VirtualPanelDocument CreateDefault()
    {
        PanelLayout[] presets = CreatePresets();
        return new(presets[0].Id, presets);
    }

    public static PanelLayout[] CreatePresets()
    {
        static PanelAction Key(string name) => new PanelKeys([KeyIdentity.Logical(name)]);
        static RuleTrigger Select(int digit) => new KeyChordTrigger(KeyIdentity.Logical("LeftAlt"), KeyIdentity.Logical($"Digit{digit}"));
        static PanelRegion Cell(int index, string name, PanelAction action, double x, double y, double width, double height) =>
            new($"cell-{index}", name, action, [PanelGeometryEditor.Rectangle(x, y, x + width, y + height)]);

        string[] digits = ["7", "8", "9", "÷", "4", "5", "6", "×", "1", "2", "3", "−"];
        PanelAction[] actions =
        [
            new PanelDigit(7), new PanelDigit(8), new PanelDigit(9), new PanelArithmetic(PanelOperator.Divide),
            new PanelDigit(4), new PanelDigit(5), new PanelDigit(6), new PanelArithmetic(PanelOperator.Multiply),
            new PanelDigit(1), new PanelDigit(2), new PanelDigit(3), new PanelArithmetic(PanelOperator.Subtract),
        ];
        PanelRegion[] numpad =
        [
            .. digits.Select((label, i) => Cell(i, label, actions[i], i % 4 * .25, i / 4 * .25, .25, .25)),
            Cell(12, "0", new PanelDigit(0), 0, .75, .5, .25),
            Cell(13, "Enter", Key("Enter"), .5, .75, .25, .25),
            Cell(14, "+", new PanelArithmetic(PanelOperator.Add), .75, .75, .25, .25),
        ];
        (string Label, string Key)[] media =
        [
            ("Previous", "MediaPreviousTrack"), ("Play/Pause", "MediaPlayPause"), ("Next", "MediaNextTrack"),
            ("Volume −", "VolumeDown"), ("Mute", "VolumeMute"), ("Volume +", "VolumeUp"),
        ];
        var gap = new PanelPoint(.005, .005);
        return
        [
            new("numpad", "Numpad", Select(1), new(gap, numpad)),
            new("media", "Media", Select(2), new(gap, media.Select((item, i) =>
                Cell(i, item.Label, Key(item.Key), (i % 3) / 3d, (i / 3) * .5, 1 / 3d, .5)).ToArray())),
            new("slides", "Presentation controls", Select(3), new(gap,
                [Cell(0, "Previous slide", Key("PageUp"), 0, 0, .5, 1), Cell(1, "Next slide", Key("PageDown"), .5, 0, .5, 1)])),
        ];
    }

    internal static VirtualPanelDocument Parse(JsonElement panel) => new(
        panel.GetProperty("default_layout").GetString(),
        panel.GetProperty("layouts").EnumerateArray().Select(layout =>
            new PanelLayout(
                Text(layout, "id"), Text(layout, "name"),
                layout.GetProperty("select") is { ValueKind: JsonValueKind.Object } binding
                    ? ConfigCodec.ParseTrigger(binding, 8) : null,
                ParseGeometry(layout.GetProperty("geometry"), ParseAction))).ToArray());

    public static PanelGeometry ParseGeometry(JsonElement geometry, Func<JsonElement, PanelAction?> readAction) => new(
        ReadPoint(geometry.GetProperty("gap")),
        geometry.GetProperty("regions").EnumerateArray().Select(region => new PanelRegion(
            Text(region, "id"), Text(region, "name"),
            region.GetProperty("action") is { ValueKind: not JsonValueKind.Null } action ? readAction(action) : null,
            region.GetProperty("faces").EnumerateArray()
                .Select(face => (IReadOnlyList<PanelPoint>)face.EnumerateArray().Select(ReadPoint).ToArray()).ToArray()
        )).ToArray());

    public static PanelAction ParseAction(JsonElement action) => Text(action, "type") switch
    {
        "digit" => new PanelDigit(action.GetProperty("digit").GetInt32()),
        "operator" => new PanelArithmetic(Enum.Parse<PanelOperator>(Text(action, "operator"), ignoreCase: true)),
        "key_chord" => new PanelKeys(action.GetProperty("keys").EnumerateArray().Select(ConfigCodec.ParseKey).ToArray()),
        _ => throw new FormatException("Unknown panel action"),
    };

    internal void Write(Utf8JsonWriter writer)
    {
        writer.WriteString("default_layout", DefaultLayout);
        writer.WriteStartArray("layouts");
        foreach (PanelLayout layout in Layouts)
        {
            writer.WriteStartObject();
            writer.WriteString("id", layout.Id);
            writer.WriteString("name", layout.Name);
            writer.WritePropertyName("select");
            if (layout.Select is { } binding) ConfigCodec.WriteTrigger(writer, binding);
            else writer.WriteNullValue();
            writer.WriteStartObject("geometry");
            writer.WritePropertyName("gap");
            WritePoint(writer, layout.Geometry.Gap);
            writer.WriteStartArray("regions");
            foreach (PanelRegion region in layout.Geometry.Regions)
            {
                writer.WriteStartObject();
                writer.WriteString("id", region.Id);
                writer.WriteString("name", region.Name);
                writer.WritePropertyName("action");
                WriteAction(writer, region.Action);
                writer.WriteStartArray("faces");
                foreach (IReadOnlyList<PanelPoint> face in region.Faces)
                {
                    writer.WriteStartArray();
                    foreach (PanelPoint point in face) WritePoint(writer, point);
                    writer.WriteEndArray();
                }
                writer.WriteEndArray();
                writer.WriteEndObject();
            }
            writer.WriteEndArray();
            writer.WriteEndObject();
            writer.WriteEndObject();
        }
        writer.WriteEndArray();
    }

    private static void WriteAction(Utf8JsonWriter writer, PanelAction? action)
    {
        if (action is null)
        {
            writer.WriteNullValue();
            return;
        }
        writer.WriteStartObject();
        switch (action)
        {
            case PanelDigit digit:
                writer.WriteString("type", "digit");
                writer.WriteNumber("digit", digit.Digit);
                break;
            case PanelArithmetic arithmetic:
                writer.WriteString("type", "operator");
                writer.WriteString("operator", arithmetic.Operator.ToString().ToLowerInvariant());
                break;
            case PanelKeys keys:
                writer.WriteString("type", "key_chord");
                writer.WriteStartArray("keys");
                foreach (KeyIdentity key in keys.Keys) ConfigCodec.WriteKey(writer, key);
                writer.WriteEndArray();
                break;
        }
        writer.WriteEndObject();
    }

    private static string Text(JsonElement value, string field) =>
        value.GetProperty(field).GetString() ?? throw new FormatException($"Missing panel {field}");

    private static PanelPoint ReadPoint(JsonElement value) => new(value[0].GetDouble(), value[1].GetDouble());

    private static void WritePoint(Utf8JsonWriter writer, PanelPoint point)
    {
        writer.WriteStartArray();
        writer.WriteNumberValue(point.X);
        writer.WriteNumberValue(point.Y);
        writer.WriteEndArray();
    }
}
