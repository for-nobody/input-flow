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
    string? LastError)
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
            : value.GetProperty("last_error").GetString());

    private static string RequiredString(JsonElement value, string name) =>
        value.GetProperty(name).GetString() ?? throw new FormatException($"{name} must be a string");
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
