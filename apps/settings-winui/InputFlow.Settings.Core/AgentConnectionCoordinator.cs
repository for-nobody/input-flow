using System.Text.Json;
using InputFlow.Protocol;

namespace InputFlow.Settings.Core;

public interface IAgentGateway
{
    Task<ConfigDocument> GetConfigAsync(CancellationToken cancellationToken = default);

    Task<AgentStatus> GetStatusAsync(CancellationToken cancellationToken = default);

    Task<JsonElement> ValidateConfigAsync(
        ConfigDocument config,
        CancellationToken cancellationToken = default);

    Task<JsonElement> ApplyConfigAsync(
        ConfigDocument config,
        CancellationToken cancellationToken = default);

    Task ReconnectAsync(CancellationToken cancellationToken = default);
}

public sealed class AgentConnectionCoordinator : IAgentGateway, IAsyncDisposable
{
    private readonly SemaphoreSlim _connectGate = new(1, 1);
    private readonly SemaphoreSlim _controlGate = new(1, 1);
    private readonly SemaphoreSlim _eventReconnectGate = new(1, 1);
    private readonly CancellationTokenSource _lifetime = new();
    private readonly object _stateLock = new();
    private readonly CaptureSessionTracker _capture = new();
    private readonly EventSequenceTracker _eventSequence = new();
    private InputFlowClient? _controlClient;
    private InputFlowClient? _eventClient;
    private InputFlowEventSubscription? _subscription;
    private Task? _eventTask;
    private long _generation;
    private bool _disposed;

    public event EventHandler? AuthorityChanged;

    public event EventHandler<AgentConnectionSnapshot>? ConnectionChanged;

    public event EventHandler<CaptureTerminal>? CaptureCompleted;

    public AgentConnectionSnapshot Connection { get; private set; } = new(
        AgentConnectionKind.Offline,
        "尚未连接 Agent",
        false,
        false);

    public AgentStatus? LatestStatus { get; private set; }

    public ConfigDocument? LatestConfig { get; private set; }

    public HandshakeInfo? Handshake { get; private set; }

    public ulong? ActiveCaptureSession => _capture.ActiveSession;

    public async Task ConnectAsync(CancellationToken cancellationToken = default)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        await _connectGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            SetConnection(AgentConnectionKind.Connecting, "正在连接 InputFlow Agent…", false, false);
            long generation = Interlocked.Increment(ref _generation);
            await DropConnectionsAsync().ConfigureAwait(false);

            InputFlowClient? control = null;
            InputFlowClient? eventClient = null;
            InputFlowEventSubscription? subscription = null;
            try
            {
                control = await InputFlowClient.ConnectAsync(cancellationToken: cancellationToken)
                    .ConfigureAwait(false);
                RequireCompatibleHandshake(control.Handshake);
                JsonElement statusJson = await control.GetStatusAsync(cancellationToken).ConfigureAwait(false);
                JsonElement configJson = await control.GetConfigAsync(cancellationToken).ConfigureAwait(false);

                eventClient = await InputFlowClient.ConnectAsync(cancellationToken: cancellationToken)
                    .ConfigureAwait(false);
                RequireCompatibleHandshake(eventClient.Handshake);
                subscription = await eventClient.SubscribeEventsAsync(cancellationToken).ConfigureAwait(false);

                _controlClient = control;
                _eventClient = eventClient;
                _subscription = subscription;
                Handshake = control.Handshake;
                LatestStatus = AgentStatus.Parse(statusJson);
                LatestConfig = ConfigCodec.Parse(configJson.GetProperty("config"));
                _eventSequence.Reset();
                SetConnection(AgentConnectionKind.Online, "已连接 Agent", true, true);
                AuthorityChanged?.Invoke(this, EventArgs.Empty);
                _eventTask = ReadEventsAsync(generation, subscription, _lifetime.Token);
            }
            catch (Exception error)
            {
                if (subscription is not null)
                {
                    await subscription.DisposeAsync().ConfigureAwait(false);
                }
                else if (eventClient is not null)
                {
                    await eventClient.DisposeAsync().ConfigureAwait(false);
                }

                if (control is not null)
                {
                    await control.DisposeAsync().ConfigureAwait(false);
                }

                SetConnection(AgentConnectionKind.Offline, ConnectionMessage(error), false, false);
                throw;
            }
        }
        finally
        {
            _connectGate.Release();
        }
    }

    public Task ReconnectAsync(CancellationToken cancellationToken = default) =>
        ConnectAsync(cancellationToken);

    public async Task<AgentStatus> GetStatusAsync(CancellationToken cancellationToken = default)
    {
        JsonElement result = await SendControlAsync(
            client => client.GetStatusAsync(cancellationToken)).ConfigureAwait(false);
        LatestStatus = AgentStatus.Parse(result);
        AuthorityChanged?.Invoke(this, EventArgs.Empty);
        return LatestStatus;
    }

    public async Task<ConfigDocument> GetConfigAsync(CancellationToken cancellationToken = default)
    {
        JsonElement result = await SendControlAsync(
            client => client.GetConfigAsync(cancellationToken)).ConfigureAwait(false);
        LatestConfig = ConfigCodec.Parse(result.GetProperty("config"));
        AuthorityChanged?.Invoke(this, EventArgs.Empty);
        return LatestConfig.DeepCopy();
    }

    public Task<JsonElement> ValidateConfigAsync(
        ConfigDocument config,
        CancellationToken cancellationToken = default)
    {
        JsonElement payload = ConfigCodec.ToJsonElement(config);
        return SendControlAsync(client => client.ValidateConfigAsync(payload, cancellationToken));
    }

    public Task<JsonElement> ApplyConfigAsync(
        ConfigDocument config,
        CancellationToken cancellationToken = default)
    {
        JsonElement payload = ConfigCodec.ToJsonElement(config);
        return SendControlAsync(client => client.ApplyConfigAsync(payload, cancellationToken));
    }

    public async Task<AgentStatus> PauseAsync(CancellationToken cancellationToken = default)
    {
        JsonElement result = await SendControlAsync(
            client => client.PauseAsync(cancellationToken)).ConfigureAwait(false);
        if (!result.GetProperty("suspended").GetBoolean())
        {
            throw new ProtocolException("pause_not_confirmed", "Agent did not confirm pause");
        }

        return await GetStatusAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task<AgentStatus> ResumeAsync(CancellationToken cancellationToken = default)
    {
        JsonElement result = await SendControlAsync(
            client => client.ResumeAsync(cancellationToken)).ConfigureAwait(false);
        if (result.GetProperty("suspended").GetBoolean())
        {
            throw new ProtocolException("resume_not_confirmed", "Agent remained suspended");
        }

        return await GetStatusAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task<AgentStats> GetStatsAsync(CancellationToken cancellationToken = default)
    {
        JsonElement result = await SendControlAsync(
            client => client.GetStatsAsync(cancellationToken)).ConfigureAwait(false);
        return AgentStats.Parse(result);
    }

    public async Task<CaptureStarted> BeginCaptureAsync(
        int timeoutMilliseconds = 10_000,
        CancellationToken cancellationToken = default)
    {
        if (!Connection.EventStreamConnected)
        {
            throw new InvalidOperationException("录制需要可用的事件订阅连接，请先重试连接");
        }

        _capture.Begin();

        try
        {
            JsonElement result = await SendControlAsync(
                client => client.BeginCaptureAsync(timeoutMilliseconds, cancellationToken))
                .ConfigureAwait(false);
            ulong sessionId = result.GetProperty("session_id").GetUInt64();
            int timeout = result.GetProperty("timeout_ms").GetInt32();
            CaptureTerminal? buffered = _capture.CompleteBegin(sessionId);
            return new CaptureStarted(sessionId, timeout, buffered);
        }
        catch
        {
            _capture.FailBegin();
            throw;
        }
    }

    public async Task CancelCaptureAsync(
        ulong sessionId,
        CancellationToken cancellationToken = default)
    {
        // Invalidate before sending cancel so a capture event racing with the
        // UI's cancel intent can never update an editor field.
        if (!_capture.InvalidateForCancel(sessionId))
        {
            return;
        }

        await SendControlAsync(
            client => client.CancelCaptureAsync(sessionId, cancellationToken)).ConfigureAwait(false);
    }

    private async Task<JsonElement> SendControlAsync(Func<InputFlowClient, Task<JsonElement>> operation)
    {
        await _controlGate.WaitAsync(_lifetime.Token).ConfigureAwait(false);
        InputFlowClient? client = _controlClient;
        if (client is null)
        {
            _controlGate.Release();
            throw new InvalidOperationException("InputFlow Agent 未连接");
        }

        try
        {
            return await operation(client).ConfigureAwait(false);
        }
        catch (Exception error)
        {
            if (ReferenceEquals(_controlClient, client))
            {
                _controlClient = null;
                SetConnection(AgentConnectionKind.Offline, ConnectionMessage(error), false, false);
                CompleteCaptureAsIndeterminate("控制连接已断开，录制结果无法确定");
            }

            await client.DisposeAsync().ConfigureAwait(false);
            throw;
        }
        finally
        {
            _controlGate.Release();
        }
    }

    private async Task ReadEventsAsync(
        long generation,
        InputFlowEventSubscription subscription,
        CancellationToken cancellationToken)
    {
        try
        {
            while (!cancellationToken.IsCancellationRequested && generation == Volatile.Read(ref _generation))
            {
                JsonElement? next = await subscription.ReadAsync(cancellationToken).ConfigureAwait(false);
                if (next is null)
                {
                    throw new EndOfStreamException("Agent closed the event stream");
                }

                await HandleEventAsync(generation, next.Value, cancellationToken).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception error)
        {
            if (generation != Volatile.Read(ref _generation) || _disposed)
            {
                return;
            }

            SetConnection(
                AgentConnectionKind.EventStreamReconnecting,
                $"事件订阅已断开，正在重连：{error.Message}",
                _controlClient is not null,
                false);
            CompleteCaptureAsIndeterminate("事件订阅已断开，无法确定录制结果，请重新录制");
            try
            {
                await subscription.DisposeAsync().ConfigureAwait(false);
            }
            catch
            {
            }
            if (ReferenceEquals(_subscription, subscription))
            {
                _subscription = null;
                _eventClient = null;
            }
            _ = RecoverEventStreamAsync(generation, _lifetime.Token);
        }
    }

    private async Task HandleEventAsync(
        long generation,
        JsonElement envelope,
        CancellationToken cancellationToken)
    {
        ulong eventId = envelope.GetProperty("event_id").GetUInt64();
        bool gap = _eventSequence.Observe(eventId);
        string type = envelope.GetProperty("type").GetString() ?? string.Empty;

        if (gap)
        {
            await RefreshAuthorityAsync(includeConfig: true, cancellationToken).ConfigureAwait(false);
        }

        switch (type)
        {
            case "status_changed":
                await RefreshAuthorityAsync(includeConfig: false, cancellationToken).ConfigureAwait(false);
                break;
            case "config_applied":
                await RefreshAuthorityAsync(includeConfig: true, cancellationToken).ConfigureAwait(false);
                break;
            case "capture_completed":
                PublishCapture(ParseCapture(envelope));
                await RefreshAuthorityAsync(includeConfig: false, cancellationToken).ConfigureAwait(false);
                break;
            case "server_shutting_down":
                SetConnection(AgentConnectionKind.Offline, "Agent 正在退出", false, false);
                CompleteCaptureAsIndeterminate("Agent 已退出，录制未完成");
                Interlocked.Increment(ref _generation);
                break;
            case "heartbeat":
                break;
            default:
                throw new ProtocolException("invalid_event", $"Unknown event type `{type}`");
        }

        if (generation != Volatile.Read(ref _generation))
        {
            return;
        }
    }

    private async Task RecoverEventStreamAsync(long generation, CancellationToken cancellationToken)
    {
        await _eventReconnectGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            while (!cancellationToken.IsCancellationRequested &&
                   generation == Volatile.Read(ref _generation) &&
                   _controlClient is not null)
            {
                InputFlowClient? client = null;
                try
                {
                    client = await InputFlowClient.ConnectAsync(cancellationToken: cancellationToken)
                        .ConfigureAwait(false);
                    RequireCompatibleHandshake(client.Handshake);
                    InputFlowEventSubscription subscription = await client.SubscribeEventsAsync(cancellationToken)
                        .ConfigureAwait(false);
                    _eventClient = client;
                    _subscription = subscription;
                    _eventSequence.Reset();
                    SetConnection(AgentConnectionKind.Online, "事件订阅已恢复", true, true);
                    await RefreshAuthorityAsync(includeConfig: true, cancellationToken).ConfigureAwait(false);
                    _eventTask = ReadEventsAsync(generation, subscription, cancellationToken);
                    return;
                }
                catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
                {
                    if (client is not null)
                    {
                        await client.DisposeAsync().ConfigureAwait(false);
                    }

                    return;
                }
                catch
                {
                    if (client is not null)
                    {
                        await client.DisposeAsync().ConfigureAwait(false);
                    }

                    await Task.Delay(TimeSpan.FromSeconds(1), cancellationToken).ConfigureAwait(false);
                }
            }
        }
        finally
        {
            _eventReconnectGate.Release();
        }
    }

    private async Task RefreshAuthorityAsync(bool includeConfig, CancellationToken cancellationToken)
    {
        await GetStatusAsync(cancellationToken).ConfigureAwait(false);
        if (includeConfig)
        {
            await GetConfigAsync(cancellationToken).ConfigureAwait(false);
        }
    }

    private void PublishCapture(CaptureTerminal terminal)
    {
        if (_capture.Observe(terminal))
        {
            CaptureCompleted?.Invoke(this, terminal);
        }
    }

    private void CompleteCaptureAsIndeterminate(string message)
    {
        CaptureTerminal? terminal = _capture.EventStreamLost(message);

        if (terminal is not null)
        {
            CaptureCompleted?.Invoke(this, terminal);
        }
    }

    private static CaptureTerminal ParseCapture(JsonElement envelope)
    {
        ulong sessionId = envelope.GetProperty("session_id").GetUInt64();
        JsonElement outcome = envelope.GetProperty("outcome");
        string kind = outcome.GetProperty("outcome").GetString() ?? string.Empty;
        if (kind == "captured")
        {
            JsonElement input = outcome.GetProperty("input");
            string inputType = input.GetProperty("input_type").GetString() ?? string.Empty;
            CapturedInput captured = inputType switch
            {
                "key" => ParseCapturedKey(input),
                "mouse_button" => new CapturedMouseButton(
                    input.GetProperty("button").GetString()
                        ?? throw new FormatException("captured mouse button is missing")),
                _ => throw new FormatException($"Unknown captured input type `{inputType}`"),
            };
            return new CaptureTerminal(sessionId, CaptureTerminalKind.Captured, captured, "已录制输入");
        }

        return kind switch
        {
            "cancelled" => new CaptureTerminal(sessionId, CaptureTerminalKind.Cancelled, null, "录制已取消"),
            "timed_out" => new CaptureTerminal(sessionId, CaptureTerminalKind.TimedOut, null, "录制已超时"),
            "shutdown" => new CaptureTerminal(sessionId, CaptureTerminalKind.AgentShutdown, null, "Agent 已退出"),
            _ => throw new FormatException($"Unknown capture outcome `{kind}`"),
        };
    }

    private static CapturedKey ParseCapturedKey(JsonElement input)
    {
        JsonElement physical = input.GetProperty("physical");
        return new CapturedKey(
            input.GetProperty("logical").GetString()
                ?? throw new FormatException("captured logical key is missing"),
            physical.ValueKind == JsonValueKind.Null
                ? null
                : physical.GetProperty("scan_code").GetUInt16(),
            physical.ValueKind != JsonValueKind.Null && physical.GetProperty("extended").GetBoolean());
    }

    private static void RequireCompatibleHandshake(HandshakeInfo handshake)
    {
        if (handshake.ProtocolVersion != ProtocolConstants.Version)
        {
            throw new ProtocolException(
                "unsupported_protocol",
                $"设置程序需要协议 v{ProtocolConstants.Version}，Agent 提供 v{handshake.ProtocolVersion}");
        }

        if (handshake.SchemaVersion != ConfigDocument.CurrentSchemaVersion ||
            !handshake.Capabilities.Contains("config_v4", StringComparer.Ordinal))
        {
            throw new ProtocolException(
                "unsupported_schema",
                $"设置程序需要配置 Schema v{ConfigDocument.CurrentSchemaVersion}，Agent 提供 v{handshake.SchemaVersion}");
        }
    }

    private void SetConnection(
        AgentConnectionKind kind,
        string message,
        bool controlConnected,
        bool eventConnected)
    {
        lock (_stateLock)
        {
            Connection = new AgentConnectionSnapshot(kind, message, controlConnected, eventConnected);
        }

        ConnectionChanged?.Invoke(this, Connection);
    }

    private static string ConnectionMessage(Exception error) => error switch
    {
        TimeoutException => $"连接或请求超时：{error.Message}",
        UnauthorizedAccessException => "无权连接 Agent。请确认设置程序与 Agent 位于同一用户会话。",
        ProtocolException protocol => $"协议不兼容（{protocol.Code}）：{protocol.Message}",
        _ => $"Agent 离线：{error.Message}",
    };

    private async Task DropConnectionsAsync()
    {
        InputFlowEventSubscription? subscription = _subscription;
        InputFlowClient? eventClient = _eventClient;
        InputFlowClient? control = _controlClient;
        _subscription = null;
        _eventClient = null;
        _controlClient = null;

        if (subscription is not null)
        {
            await subscription.DisposeAsync().ConfigureAwait(false);
        }
        else if (eventClient is not null)
        {
            await eventClient.DisposeAsync().ConfigureAwait(false);
        }

        if (control is not null)
        {
            await control.DisposeAsync().ConfigureAwait(false);
        }
    }

    public async ValueTask DisposeAsync()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        if (ActiveCaptureSession is ulong sessionId)
        {
            using var cancelDeadline = new CancellationTokenSource(TimeSpan.FromMilliseconds(500));
            try
            {
                await CancelCaptureAsync(sessionId, cancelDeadline.Token).ConfigureAwait(false);
            }
            catch
            {
                // The control connection owner disconnect below is the agent's
                // final bounded capture-cancellation guarantee.
            }
        }

        _lifetime.Cancel();
        Interlocked.Increment(ref _generation);
        await DropConnectionsAsync().ConfigureAwait(false);
        if (_eventTask is not null)
        {
            try
            {
                await _eventTask.ConfigureAwait(false);
            }
            catch
            {
            }
        }

        _lifetime.Dispose();
        _connectGate.Dispose();
        _controlGate.Dispose();
        _eventReconnectGate.Dispose();
    }
}
