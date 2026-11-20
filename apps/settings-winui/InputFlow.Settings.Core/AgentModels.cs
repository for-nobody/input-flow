using System.Text.Json;

namespace InputFlow.Settings.Core;

public enum AgentConnectionKind
{
    Offline,
    Connecting,
    Online,
    EventStreamReconnecting,
}

public sealed record AgentConnectionSnapshot(
    AgentConnectionKind Kind,
    string Message,
    bool ControlConnected,
    bool EventStreamConnected);

public sealed record AgentStatus(
    string Phase,
    bool Suspended,
    bool CaptureActive,
    int RuleCount,
    string EmergencyKey,
    ulong StateRevision,
    string ApplyReconciliation,
    ulong? ReconciliationRequestId,
    string? LastError,
    TouchpadStatus? Touchpad = null,
    PresentationStatus? Presentation = null,
    NumpadStatus? Numpad = null)
{
    public static AgentStatus Parse(JsonElement value) => new(
        RequiredString(value, "phase"),
        value.GetProperty("suspended").GetBoolean(),
        value.GetProperty("capture_active").GetBoolean(),
        value.GetProperty("rule_count").GetInt32(),
        RequiredString(value, "emergency_key"),
        value.GetProperty("state_revision").GetUInt64(),
        RequiredString(value, "apply_reconciliation"),
        value.GetProperty("reconciliation_request_id").ValueKind == JsonValueKind.Null
            ? null
            : value.GetProperty("reconciliation_request_id").GetUInt64(),
        value.GetProperty("last_error").ValueKind == JsonValueKind.Null
            ? null
            : value.GetProperty("last_error").GetString(),
        value.TryGetProperty("touchpad", out JsonElement touchpad) && touchpad.ValueKind == JsonValueKind.Object
            ? TouchpadStatus.Parse(touchpad)
            : null,
        value.TryGetProperty("presentation", out JsonElement presentation) && presentation.ValueKind == JsonValueKind.Object
            ? PresentationStatus.Parse(presentation)
            : null,
        value.TryGetProperty("numpad", out JsonElement numpad) && numpad.ValueKind == JsonValueKind.Object
            ? NumpadStatus.Parse(numpad)
            : null);

    private static string RequiredString(JsonElement value, string name) =>
        value.GetProperty(name).GetString() ?? throw new FormatException($"{name} must be a string");
}

public sealed record NumpadStatus(string Phase, string Reason, ulong Confirmed, ulong Cancelled, ulong InsertedInputs)
{
    public string? ActiveLayout { get; init; }
    public double? WidthMm { get; init; }
    public double? HeightMm { get; init; }
    public static NumpadStatus Parse(JsonElement value) => new(
        value.GetProperty("phase").GetString() ?? "unknown",
        value.GetProperty("reason").GetString() ?? "unknown",
        value.GetProperty("confirmed").GetUInt64(), value.GetProperty("cancelled").GetUInt64(),
        value.GetProperty("inserted_inputs").GetUInt64())
    {
        ActiveLayout = value.TryGetProperty("active_layout", out JsonElement layout) ? layout.GetString() : null,
        WidthMm = ReadMillimeters(value, 0),
        HeightMm = ReadMillimeters(value, 1),
    };

    private static double? ReadMillimeters(JsonElement value, int axis) =>
        value.TryGetProperty("size_um", out JsonElement size) && size.ValueKind == JsonValueKind.Array
        && size.GetArrayLength() == 2 ? size[axis].GetUInt32() / 1000.0 : null;
}

public sealed record PresentationDisplay(string Id, int Left, int Top, int Width, int Height, bool Primary);

public sealed record PresentationStatus(
    bool Enabled, bool CanStart, string Phase, string Reason, string? Preset, int Segments,
    IReadOnlyList<PresentationDisplay> Displays)
{
    public static PresentationStatus Parse(JsonElement value) => new(
        value.GetProperty("enabled").GetBoolean(), value.GetProperty("can_start").GetBoolean(),
        value.GetProperty("phase").GetString() ?? "", value.GetProperty("reason").GetString() ?? "",
        value.GetProperty("preset").GetString(), value.GetProperty("segments").GetInt32(),
        value.GetProperty("displays").EnumerateArray().Select(display => new PresentationDisplay(
            display.GetProperty("id").GetString() ?? "", display.GetProperty("left").GetInt32(), display.GetProperty("top").GetInt32(),
            display.GetProperty("width").GetInt32(), display.GetProperty("height").GetInt32(), display.GetProperty("primary").GetBoolean()
        )).ToArray());
}

public sealed record TouchpadStatus(
    bool ExperimentalEnabled, bool ClickEnabled, bool DeviceAvailable, bool OutputAllowed,
    string Reason, ulong Reports, ulong SinkReports, ulong CompleteContacts, ulong ShortIntents, ulong LongIntents,
    ulong RequestedInputs, ulong InsertedInputs, string? LastCancel, bool NormalizedDistance)
{
    public static TouchpadStatus Parse(JsonElement value) => new(
        value.GetProperty("experimental_enabled").GetBoolean(),
        value.GetProperty("click_enabled").GetBoolean(),
        value.GetProperty("device_available").GetBoolean(),
        value.GetProperty("output_allowed").GetBoolean(),
        value.GetProperty("reason").GetString() ?? "unknown",
        value.GetProperty("reports").GetUInt64(),
        value.GetProperty("sink_reports").GetUInt64(),
        value.GetProperty("complete_contacts").GetUInt64(),
        value.GetProperty("short_intents").GetUInt64(),
        value.GetProperty("long_intents").GetUInt64(),
        value.GetProperty("requested_inputs").GetUInt64(),
        value.GetProperty("inserted_inputs").GetUInt64(),
        value.GetProperty("last_cancel").GetString(),
        value.GetProperty("normalized_distance").GetBoolean());
}

public sealed record PercentileSummary(
    ulong Samples,
    ulong? P50,
    ulong? P95,
    ulong? P99,
    ulong? Max)
{
    internal static PercentileSummary? ParseOptional(JsonElement value, string name)
    {
        JsonElement element = value.GetProperty(name);
        if (element.ValueKind == JsonValueKind.Null)
        {
            return null;
        }

        return new PercentileSummary(
            element.GetProperty("samples").GetUInt64(),
            OptionalUInt64(element, "p50"),
            OptionalUInt64(element, "p95"),
            OptionalUInt64(element, "p99"),
            OptionalUInt64(element, "max"));
    }

    private static ulong? OptionalUInt64(JsonElement value, string name) =>
        value.GetProperty(name).ValueKind == JsonValueKind.Null
            ? null
            : value.GetProperty(name).GetUInt64();
}

public sealed record AgentStats(
    ulong ObservedEvents,
    ulong OutputBatchesSent,
    ulong OutputBatchesFailed,
    ulong OutputBatchesDropped,
    PercentileSummary? CallbackLatencyMicroseconds,
    PercentileSummary? HoldDelayMicroseconds)
{
    public static AgentStats Parse(JsonElement value) => new(
        value.GetProperty("observed_events").GetUInt64(),
        value.GetProperty("output_batches_sent").GetUInt64(),
        value.GetProperty("output_batches_failed").GetUInt64(),
        value.GetProperty("output_batches_dropped").GetUInt64(),
        PercentileSummary.ParseOptional(value, "callback_latency_us"),
        PercentileSummary.ParseOptional(value, "hold_delay_us"));
}

public abstract record CapturedInput;

public sealed record CapturedKey(
    string Logical,
    ushort? ScanCode,
    bool Extended) : CapturedInput;

public sealed record CapturedMouseButton(string Button) : CapturedInput;

public enum CaptureTerminalKind
{
    Captured,
    Cancelled,
    TimedOut,
    AgentShutdown,
    EventStreamLost,
}

public sealed record CaptureTerminal(
    ulong SessionId,
    CaptureTerminalKind Kind,
    CapturedInput? Input,
    string Message);

public sealed record CaptureStarted(
    ulong SessionId,
    int TimeoutMilliseconds,
    CaptureTerminal? CompletedBeforeResponse = null);
