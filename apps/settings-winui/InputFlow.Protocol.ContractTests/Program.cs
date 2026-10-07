using System.Text.Json;
using InputFlow.Protocol;

string root = FindRepositoryRoot(AppContext.BaseDirectory);
string fixtureDirectory = Path.Combine(root, "fixtures", "protocol", "v1");
string[] fixtures =
[
    "handshake-request.json",
    "handshake-response.json",
    "get-config-response.json",
    "error-response.json",
    "capture-event.json",
];

foreach (string fixture in fixtures)
{
    string path = Path.Combine(fixtureDirectory, fixture);
    using JsonDocument document = JsonDocument.Parse(await File.ReadAllBytesAsync(path));
    await using var stream = new MemoryStream();
    await ProtocolFrame.WriteAsync(stream, document.RootElement);
    stream.Position = 0;
    using JsonDocument? decoded = await ProtocolFrame.ReadAsync(stream);
    if (decoded is null)
    {
        throw new InvalidOperationException($"{fixture} did not decode");
    }
    Assert(
        JsonElement.DeepEquals(document.RootElement, decoded.RootElement),
        $"{fixture} changed during C# framing round-trip");
}

using (JsonDocument response = JsonDocument.Parse(
    await File.ReadAllBytesAsync(Path.Combine(fixtureDirectory, "get-config-response.json"))))
using (JsonDocument config = JsonDocument.Parse(
    await File.ReadAllBytesAsync(Path.Combine(root, "fixtures", "config", "v4-valid.json"))))
{
    JsonElement embedded = response.RootElement.GetProperty("result").GetProperty("config");
    Assert(
        JsonElement.DeepEquals(embedded, config.RootElement),
        "protocol get_config golden does not contain the shared schema-v4 golden config");
}

Console.WriteLine($"InputFlow.Protocol contract tests passed: {fixtures.Length + 1}");

if (args.Contains("--live", StringComparer.Ordinal))
{
    await RunLiveContractAsync();
    Console.WriteLine("InputFlow.Protocol live agent contract passed");
}

static async Task RunLiveContractAsync()
{
    await using InputFlowClient client = await InputFlowClient.ConnectAsync();
    JsonElement status = await client.GetStatusAsync();
    Assert(status.GetProperty("phase").GetString() == "ready", "live status is not ready");

    JsonElement configResult = await client.GetConfigAsync();
    JsonElement config = configResult.GetProperty("config");
    JsonElement validation = await client.ValidateConfigAsync(config);
    Assert(validation.GetProperty("valid").GetBoolean(), "live validation rejected current config");

    JsonElement apply = await client.ApplyConfigAsync(config);
    Assert(apply.GetProperty("outcome").GetString() == "applied", "live apply did not commit");

    InputFlowClient eventClient = await InputFlowClient.ConnectAsync();
    await using InputFlowEventSubscription subscription = await eventClient.SubscribeEventsAsync();
    JsonElement paused = await client.PauseAsync();
    Assert(paused.GetProperty("suspended").GetBoolean(), "live pause did not suspend");
    using (var eventDeadline = new CancellationTokenSource(TimeSpan.FromSeconds(3)))
    {
        bool sawPause = false;
        while (!sawPause)
        {
            JsonElement? next = await subscription.ReadAsync(eventDeadline.Token);
            if (next is not JsonElement eventEnvelope)
            {
                throw new InvalidOperationException(
                    "live event subscription closed before pause notification");
            }

            sawPause = eventEnvelope.GetProperty("type").GetString() == "status_changed" &&
                eventEnvelope.GetProperty("suspended").GetBoolean();
        }
    }

    JsonElement resumed = await client.ResumeAsync();
    Assert(!resumed.GetProperty("suspended").GetBoolean(), "live resume remained suspended");

    JsonElement stats = await client.GetStatsAsync();
    Assert(stats.TryGetProperty("observed_events", out _), "live stats omitted observed_events");

    JsonElement capture = await client.BeginCaptureAsync(1_000);
    ulong sessionId = capture.GetProperty("session_id").GetUInt64();
    JsonElement cancelled = await client.CancelCaptureAsync(sessionId);
    Assert(cancelled.GetProperty("cancelled").GetBoolean(), "live capture did not cancel");

    InputFlowClient disconnectedOwner = await InputFlowClient.ConnectAsync();
    await disconnectedOwner.BeginCaptureAsync(5_000);
    await disconnectedOwner.DisposeAsync();
    using var disconnectDeadline = new CancellationTokenSource(TimeSpan.FromSeconds(3));
    while (true)
    {
        disconnectDeadline.Token.ThrowIfCancellationRequested();
        JsonElement afterDisconnect = await client.GetStatusAsync(disconnectDeadline.Token);
        if (!afterDisconnect.GetProperty("capture_active").GetBoolean())
        {
            break;
        }

        await Task.Delay(25, disconnectDeadline.Token);
    }
}

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
