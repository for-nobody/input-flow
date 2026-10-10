using System.Text.Json;
using InputFlow.Settings.Core;

string root = FindRepositoryRoot(AppContext.BaseDirectory);
int passed = 0;

Run("v1/v2/v3/v4 configuration contract", () =>
{
    ConfigDocument v1 = ParseFixture(root, "v1-valid.json");
    ConfigDocument v2 = ParseFixture(root, "v2-valid.json");
    ConfigDocument v3 = ParseFixture(root, "v3-valid.json");
    ConfigDocument v4 = ParseFixture(root, "v4-valid.json");
    Assert(v1.SchemaVersion == 4 && v1.Rules.All(rule => rule.Enabled), "v1 did not migrate enabled=true");
    Assert(v2.SchemaVersion == 4 && v2.Rules.All(rule => rule.Enabled), "v2 did not migrate enabled=true");
    Assert(v3.SchemaVersion == 4 && v3.Rules.All(rule => rule.Enabled), "v3 did not migrate to v4");
    Assert(ConfigCodec.DeepEquals(v2, v3), "v2 migration differs from the v3 golden document");
    ConfigDocument roundTrip = Parse(ConfigCodec.ToJsonElement(v4));
    Assert(ConfigCodec.DeepEquals(roundTrip, v4), "v4 typed round-trip changed the document");
    Assert(v4.Rules.All(rule => rule.Trigger is MouseDirectionTrigger), "v4 direction triggers were not typed");
});

Run("future configuration schema is rejected", () =>
{
    using JsonDocument source = JsonDocument.Parse(File.ReadAllBytes(
        Path.Combine(root, "fixtures", "config", "v4-valid.json")));
    var future = new Dictionary<string, JsonElement>();
    foreach (JsonProperty property in source.RootElement.EnumerateObject())
    {
        future[property.Name] = property.Value.Clone();
    }
    future["schema_version"] = JsonSerializer.SerializeToElement(ConfigDocument.CurrentSchemaVersion + 1);
    using JsonDocument document = JsonDocument.Parse(JsonSerializer.Serialize(future));

    bool rejected = false;
    try
    {
        _ = Parse(document.RootElement);
    }
    catch (FormatException error) when (error.Message.Contains("Unsupported schema_version", StringComparison.Ordinal))
    {
        rejected = true;
    }
    Assert(rejected, "a future schema was accepted by the Settings codec");
});

Run("draft never mutates formal snapshot", () =>
{
    ConfigDocument formal = ParseFixture(root, "v3-valid.json");
    var session = new DraftSession();
    session.Load(formal);
    ConfigDocument draft = session.Draft;
    draft.Rules[0] = draft.Rules[0] with { Enabled = false };
    session.ReplaceDraft(draft);
    Assert(session.IsDirty, "draft change was not marked dirty");
    Assert(session.FormalSnapshot.Rules[0].Enabled, "formal snapshot was mutated through the draft");
});

await RunAsync("save validates, applies, and reads back", async () =>
{
    ConfigDocument baseline = ParseFixture(root, "v3-valid.json");
    ConfigDocument draft = baseline.DeepCopy();
    draft.Rules[0] = draft.Rules[0] with { Enabled = false };
    var gateway = new FakeGateway(baseline);
    var service = new ConfigSaveService(gateway);
    SaveResult result = await service.SaveAsync(draft, baseline, false);
    Assert(result.Kind == SaveResultKind.Applied, result.Message);
    Assert(gateway.ValidateCalls == 1 && gateway.ApplyCalls == 1, "save did not call validate/apply exactly once");
    Assert(result.ConfirmedFormal?.EnabledRuleCount == 0, "disabled rule was counted as enabled");
});

await RunAsync("external change requires an explicit overwrite decision", async () =>
{
    ConfigDocument baseline = ParseFixture(root, "v3-valid.json");
    ConfigDocument external = baseline.DeepCopy();
    external.Rules[0] = external.Rules[0] with { Id = "changed-elsewhere" };
    var gateway = new FakeGateway(external);
    var service = new ConfigSaveService(gateway);
    SaveResult result = await service.SaveAsync(baseline, baseline, false);
    Assert(result.Kind == SaveResultKind.ExternalChangeDetected, "external edit was not detected");
    Assert(gateway.ApplyCalls == 0, "external edit detection still mutated the agent");
});

await RunAsync("validation failure preserves the draft and skips apply", async () =>
{
    ConfigDocument baseline = ParseFixture(root, "v3-valid.json");
    var gateway = new FakeGateway(baseline)
    {
        ValidationResponse = Json(new { valid = false, rule_count = 0, errors = new[] { "rules[0]: conflict" } }),
    };
    var service = new ConfigSaveService(gateway);
    SaveResult result = await service.SaveAsync(baseline, baseline, false);
    Assert(result.Kind == SaveResultKind.ValidationFailed, "validation error was not surfaced");
    Assert(result.ValidationErrors.Single().Contains("conflict", StringComparison.Ordinal), "validation details missing");
    Assert(gateway.ApplyCalls == 0, "invalid draft was applied");
});

await RunAsync("request timeout reconciles without retrying mutation", async () =>
{
    ConfigDocument baseline = ParseFixture(root, "v3-valid.json");
    ConfigDocument draft = baseline.DeepCopy();
    draft.Rules[0] = draft.Rules[0] with { Enabled = false };
    var gateway = new FakeGateway(baseline)
    {
        ApplyException = new TimeoutException("injected timeout"),
        ReconnectAction = fake =>
        {
            fake.Formal = draft.DeepCopy();
            fake.Status = ReadyStatus("applied_after_timeout", draft.EnabledRuleCount);
        },
    };
    var service = new ConfigSaveService(gateway);
    SaveResult result = await service.SaveAsync(draft, baseline, false);
    Assert(result.Kind == SaveResultKind.Applied, result.Message);
    Assert(gateway.ApplyCalls == 1, "timed-out mutation was retried");
    Assert(gateway.ReconnectCalls == 1, "timeout did not reconnect for reconciliation");
});

await RunAsync("runtime outcome unknown reports rollback and recovery distinctly", async () =>
{
    ConfigDocument baseline = ParseFixture(root, "v3-valid.json");
    var rollbackGateway = new FakeGateway(baseline)
    {
        ApplyResponse = Apply("runtime_outcome_unknown", recoveryRequired: true),
        Status = ReadyStatus("rolled_back_after_timeout", baseline.EnabledRuleCount),
    };
    SaveResult rolledBack = await new ConfigSaveService(rollbackGateway)
        .SaveAsync(baseline, baseline, false);
    Assert(rolledBack.Kind == SaveResultKind.RolledBackAfterTimeout, "rollback was not distinguished");

    var recoveryGateway = new FakeGateway(baseline)
    {
        ApplyResponse = Apply("runtime_outcome_unknown", recoveryRequired: true),
        Status = ReadyStatus("recovery_required", baseline.EnabledRuleCount),
    };
    SaveResult recovery = await new ConfigSaveService(recoveryGateway)
        .SaveAsync(baseline, baseline, false);
    Assert(recovery.Kind == SaveResultKind.RecoveryRequired, "recovery-required state was not distinguished");
});

Run("capture tracker buffers early completion and rejects stale sessions", () =>
{
    var tracker = new CaptureSessionTracker();
    tracker.Begin();
    var early = new CaptureTerminal(4, CaptureTerminalKind.Captured, new CapturedKey("A", 30, false), "ok");
    Assert(!tracker.Observe(early), "completion before begin response was published early");
    Assert(tracker.CompleteBegin(4) == early, "early completion was not delivered after session id arrived");

    tracker.Begin();
    Assert(tracker.CompleteBegin(5) is null, "unexpected buffered completion");
    Assert(tracker.InvalidateForCancel(5), "active capture did not cancel");
    Assert(!tracker.Observe(early with { SessionId = 5 }), "cancelled session result was accepted");
    tracker.Begin();
    tracker.CompleteBegin(6);
    Assert(!tracker.Observe(early with { SessionId = 5 }), "old session overwrote the new capture");
    Assert(tracker.ActiveSession == 6, "old event cleared the active session");
    Assert(tracker.Observe(early with { SessionId = 6 }), "current session result was rejected");

    tracker.Begin();
    Assert(tracker.EventStreamLost("lost") is null, "pending begin had a publishable session id");
    CaptureTerminal? invalidated = tracker.CompleteBegin(7);
    Assert(invalidated?.Kind == CaptureTerminalKind.EventStreamLost, "event loss during begin reactivated capture");
    Assert(tracker.ActiveSession is null, "invalidated begin became active");
});

Run("capture UI intent locks pending begin and carries cancellation to the session id", () =>
{
    var intent = new CaptureUiIntentTracker();
    string? target = null;
    bool StartField(string field)
    {
        if (!intent.TryBegin()) return false;
        target = field;
        return true;
    }

    Assert(StartField("first"), "first field did not acquire the capture-start lock");
    Assert(!StartField("second"), "a second field acquired the lock while begin was pending");
    Assert(target == "first", "a rejected second start replaced the first target field");
    Assert(intent.RequestCancel() is null, "pending begin exposed a session id too early");
    Assert(intent.State == CaptureUiIntentState.CancelPending, "cancel intent was not retained during begin");
    Assert(!intent.TryBegin(), "cancel-pending begin allowed another field to start");

    CaptureBeginDisposition disposition = intent.CompleteBegin(41);
    Assert(disposition == CaptureBeginDisposition.CancelImmediately, "session id did not carry the pending cancel");
    Assert(intent.State == CaptureUiIntentState.Cancelling, "known session did not enter cancelling state");
    Assert(!intent.TryAcceptTerminal(41), "terminal event raced past the user's cancel intent");
    intent.CompleteCancellation(41);
    Assert(!intent.IsInProgress && intent.TryBegin(), "capture lock was not released after cancellation");
});

Run("rule enablement binding ignores initialization and isolates user changes", () =>
{
    var writes = new List<(string Id, bool Enabled)>();
    var first = new RuleEnablementBinding("first", true, (id, enabled) => writes.Add((id, enabled)));
    var second = new RuleEnablementBinding("second", true, (id, enabled) => writes.Add((id, enabled)));

    Assert(first.Enabled && second.Enabled, "initial enabled values were not preserved");
    first.Enabled = true;
    second.Enabled = true;
    Assert(writes.Count == 0, "binding initialization wrote enabled values back to the draft");

    second.Enabled = false;
    Assert(writes.SequenceEqual(new[] { ("second", false) }), "one user change affected the wrong rule or wrote more than once");
    Assert(first.Enabled && !second.Enabled, "one row's change leaked into another row");
});

Run("event id gaps require an authority resync", () =>
{
    var tracker = new EventSequenceTracker();
    Assert(!tracker.Observe(1), "first event was incorrectly a gap");
    Assert(!tracker.Observe(2), "contiguous event was incorrectly a gap");
    Assert(tracker.Observe(5), "dropped events were not detected");
    tracker.Reset();
    Assert(!tracker.Observe(20), "new subscription inherited the previous sequence");
});

Run("UI language preference defaults safely and persists supported values", () =>
{
    string testDirectory = Path.Combine(Path.GetTempPath(), $"InputFlow-language-{Guid.NewGuid():N}");
    string preferencePath = Path.Combine(testDirectory, "ui-preferences.json");
    try
    {
        var store = new UiLanguagePreferenceStore(preferencePath);
        UiLanguagePreferenceLoadResult missing = store.Load();
        Assert(missing.Language == UiLanguagePreference.System && !missing.UsedFallback,
            "missing UI preference did not default to system");

        store.Save(UiLanguagePreference.EnglishUnitedStates);
        UiLanguagePreferenceLoadResult english = store.Load();
        Assert(english.Language == UiLanguagePreference.EnglishUnitedStates && !english.UsedFallback,
            "English UI preference did not round-trip");

        store.Save(UiLanguagePreference.ChineseSimplified);
        UiLanguagePreferenceLoadResult chinese = store.Load();
        Assert(chinese.Language == UiLanguagePreference.ChineseSimplified && !chinese.UsedFallback,
            "Simplified Chinese UI preference did not round-trip");
        Assert(Directory.GetFiles(testDirectory, "*.tmp").Length == 0,
            "atomic preference save left a temporary file behind");
    }
    finally
    {
        if (Directory.Exists(testDirectory)) Directory.Delete(testDirectory, recursive: true);
    }
});

Run("UI language preference rejects corrupt and unsupported values", () =>
{
    string testDirectory = Path.Combine(Path.GetTempPath(), $"InputFlow-language-{Guid.NewGuid():N}");
    string preferencePath = Path.Combine(testDirectory, "ui-preferences.json");
    try
    {
        Directory.CreateDirectory(testDirectory);
        File.WriteAllText(preferencePath, "{not-json");
        UiLanguagePreferenceLoadResult corrupt = new UiLanguagePreferenceStore(preferencePath).Load();
        Assert(corrupt.Language == UiLanguagePreference.System && corrupt.UsedFallback,
            "corrupt UI preference did not use the safe fallback");

        File.WriteAllText(preferencePath, "{\"language\":\"fr-FR\"}");
        UiLanguagePreferenceLoadResult unsupported = new UiLanguagePreferenceStore(preferencePath).Load();
        Assert(unsupported.Language == UiLanguagePreference.System && unsupported.UsedFallback,
            "unsupported UI preference did not use the safe fallback");
    }
    finally
    {
        if (Directory.Exists(testDirectory)) Directory.Delete(testDirectory, recursive: true);
    }
});

Console.WriteLine($"InputFlow.Settings.Core tests passed: {passed}");

void Run(string name, Action test)
{
    test();
    passed++;
    Console.WriteLine($"PASS {name}");
}

async Task RunAsync(string name, Func<Task> test)
{
    await test();
    passed++;
    Console.WriteLine($"PASS {name}");
}

static ConfigDocument ParseFixture(string repositoryRoot, string name)
{
    using JsonDocument document = JsonDocument.Parse(File.ReadAllBytes(
        Path.Combine(repositoryRoot, "fixtures", "config", name)));
    return ConfigCodec.Parse(document.RootElement);
}

static ConfigDocument Parse(JsonElement element) => ConfigCodec.Parse(element);

static JsonElement Json<T>(T value) => JsonSerializer.SerializeToElement(value, new JsonSerializerOptions
{
    PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
});

static JsonElement Apply(string outcome, bool recoveryRequired) => Json(new
{
    outcome,
    rule_count = 1,
    validation_errors = Array.Empty<string>(),
    save = (object?)null,
    runtime = (object?)null,
    rollback = (object?)null,
    runtime_request_id = 42,
    error = "injected pending apply",
    recovery_required = recoveryRequired,
});

static AgentStatus ReadyStatus(string reconciliation, int ruleCount) => new(
    "ready",
    false,
    false,
    ruleCount,
    "F12",
    1,
    reconciliation,
    reconciliation == "settled" ? null : 42,
    null);

static string FindRepositoryRoot(string start)
{
    var directory = new DirectoryInfo(start);
    while (directory is not null)
    {
        if (File.Exists(Path.Combine(directory.FullName, "Cargo.toml")) &&
            Directory.Exists(Path.Combine(directory.FullName, "fixtures")))
        {
            return directory.FullName;
        }

        directory = directory.Parent;
    }

    throw new DirectoryNotFoundException("Could not locate the InputFlow repository root");
}

static void Assert(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

sealed class FakeGateway : IAgentGateway
{
    public FakeGateway(ConfigDocument formal)
    {
        Formal = formal.DeepCopy();
        Status = TestHelpers.ReadyStatus("settled", formal.EnabledRuleCount);
    }

    public ConfigDocument Formal { get; set; }

    public AgentStatus Status { get; set; }

    public JsonElement ValidationResponse { get; set; } = TestHelpers.Json(new
    {
        valid = true,
        rule_count = 1,
        errors = Array.Empty<string>(),
    });

    public JsonElement? ApplyResponse { get; set; }

    public Exception? ApplyException { get; set; }

    public Action<FakeGateway>? ReconnectAction { get; set; }

    public int ValidateCalls { get; private set; }

    public int ApplyCalls { get; private set; }

    public int ReconnectCalls { get; private set; }

    public Task<ConfigDocument> GetConfigAsync(CancellationToken cancellationToken = default) =>
        Task.FromResult(Formal.DeepCopy());

    public Task<AgentStatus> GetStatusAsync(CancellationToken cancellationToken = default) =>
        Task.FromResult(Status);

    public Task<JsonElement> ValidateConfigAsync(
        ConfigDocument config,
        CancellationToken cancellationToken = default)
    {
        ValidateCalls++;
        return Task.FromResult(ValidationResponse);
    }

    public Task<JsonElement> ApplyConfigAsync(
        ConfigDocument config,
        CancellationToken cancellationToken = default)
    {
        ApplyCalls++;
        if (ApplyException is not null)
        {
            return Task.FromException<JsonElement>(ApplyException);
        }

        if (ApplyResponse is JsonElement response)
        {
            return Task.FromResult(response);
        }

        Formal = config.DeepCopy();
        Status = TestHelpers.ReadyStatus("settled", config.EnabledRuleCount);
        return Task.FromResult(TestHelpers.Json(new
        {
            outcome = "applied",
            rule_count = config.EnabledRuleCount,
            validation_errors = Array.Empty<string>(),
            save = new { committed = true, backup_created = false, cleanup_warnings = Array.Empty<string>() },
            runtime = (object?)null,
            rollback = (object?)null,
            runtime_request_id = (ulong?)null,
            error = (string?)null,
            recovery_required = false,
        }));
    }

    public Task ReconnectAsync(CancellationToken cancellationToken = default)
    {
        ReconnectCalls++;
        ReconnectAction?.Invoke(this);
        return Task.CompletedTask;
    }
}

static class TestHelpers
{
    public static JsonElement Json<T>(T value) => JsonSerializer.SerializeToElement(value, new JsonSerializerOptions
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    });

    public static AgentStatus ReadyStatus(string reconciliation, int ruleCount) => new(
        "ready",
        false,
        false,
        ruleCount,
        "F12",
        1,
        reconciliation,
        reconciliation == "settled" ? null : 42,
        null);
}
