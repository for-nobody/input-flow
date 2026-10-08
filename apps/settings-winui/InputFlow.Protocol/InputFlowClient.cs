using System.IO.Pipes;
using System.Text.Json;

namespace InputFlow.Protocol;

public sealed class InputFlowClient : IAsyncDisposable
{
    private readonly NamedPipeClientStream _pipe;
    private readonly SemaphoreSlim _requestGate = new(1, 1);
    private readonly TimeSpan _responseTimeout;
    private long _requestSequence;
    private bool _streaming;
    private bool _disposed;
    private bool _resourcesDisposed;

    private InputFlowClient(NamedPipeClientStream pipe, TimeSpan responseTimeout)
    {
        _pipe = pipe;
        _responseTimeout = responseTimeout;
    }

    public HandshakeInfo Handshake { get; private set; } = null!;

    public static async Task<InputFlowClient> ConnectAsync(
        string? pipeName = null,
        TimeSpan? connectTimeout = null,
        TimeSpan? responseTimeout = null,
        CancellationToken cancellationToken = default)
    {
        var pipe = new NamedPipeClientStream(
            ".",
            pipeName ?? ProtocolConstants.DefaultPipeName,
            PipeDirection.InOut,
            PipeOptions.Asynchronous);
        TimeSpan connectDeadline = connectTimeout ?? TimeSpan.FromSeconds(3);
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        linked.CancelAfter(connectDeadline);
        try
        {
            await pipe.ConnectAsync(linked.Token).ConfigureAwait(false);
            var client = new InputFlowClient(
                pipe,
                responseTimeout ?? TimeSpan.FromMilliseconds(ProtocolConstants.DefaultResponseTimeoutMilliseconds));
            JsonElement handshake = await client.SendAsync(
                "handshake",
                writer =>
                {
                    writer.WriteStartObject();
                    writer.WriteString("client_name", "InputFlow.Settings");
                    writer.WriteString("client_version", "0.1.0");
                    writer.WriteEndObject();
                },
                cancellationToken).ConfigureAwait(false);
            client.Handshake = HandshakeInfo.Parse(handshake);
            return client;
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            await pipe.DisposeAsync().ConfigureAwait(false);
            throw new TimeoutException($"Timed out connecting to the InputFlow agent after {connectDeadline}");
        }
        catch
        {
            await pipe.DisposeAsync().ConfigureAwait(false);
            throw;
        }
    }

    public Task<JsonElement> GetStatusAsync(CancellationToken cancellationToken = default) =>
        SendAsync("get_status", WriteEmptyObject, cancellationToken);

    public Task<JsonElement> GetConfigAsync(CancellationToken cancellationToken = default) =>
        SendAsync("get_config", WriteEmptyObject, cancellationToken);

    public Task<JsonElement> ValidateConfigAsync(
        JsonElement config,
        CancellationToken cancellationToken = default) =>
        SendAsync("validate_config", writer => WriteConfigParameter(writer, config), cancellationToken);

    public Task<JsonElement> ApplyConfigAsync(
        JsonElement config,
        CancellationToken cancellationToken = default) =>
        SendAsync("apply_config", writer => WriteConfigParameter(writer, config), cancellationToken);

    public Task<JsonElement> PauseAsync(CancellationToken cancellationToken = default) =>
        SendAsync("pause", WriteEmptyObject, cancellationToken);

    public Task<JsonElement> ResumeAsync(CancellationToken cancellationToken = default) =>
        SendAsync("resume", WriteEmptyObject, cancellationToken);

    public Task<JsonElement> GetStatsAsync(CancellationToken cancellationToken = default) =>
        SendAsync("get_stats", WriteEmptyObject, cancellationToken);

    public Task<JsonElement> BeginCaptureAsync(
        int timeoutMilliseconds,
        CancellationToken cancellationToken = default) =>
        SendAsync(
            "begin_capture",
            writer =>
            {
                writer.WriteStartObject();
                writer.WriteNumber("timeout_ms", timeoutMilliseconds);
                writer.WriteEndObject();
            },
            cancellationToken);

    public Task<JsonElement> CancelCaptureAsync(
        ulong sessionId,
        CancellationToken cancellationToken = default) =>
        SendAsync(
            "cancel_capture",
            writer =>
            {
                writer.WriteStartObject();
                writer.WriteNumber("session_id", sessionId);
                writer.WriteEndObject();
            },
            cancellationToken);

    public async Task<InputFlowEventSubscription> SubscribeEventsAsync(
        CancellationToken cancellationToken = default)
    {
        await SendAsync("subscribe_events", WriteEmptyObject, cancellationToken).ConfigureAwait(false);
        _streaming = true;
        return new InputFlowEventSubscription(this);
    }

    internal async ValueTask<JsonElement?> ReadEventAsync(CancellationToken cancellationToken)
    {
        ThrowIfDisposed();
        try
        {
            using JsonDocument? document = await ProtocolFrame.ReadAsync(_pipe, cancellationToken).ConfigureAwait(false);
            if (document is null)
            {
                return null;
            }

            JsonElement root = document.RootElement;
            if (root.GetProperty("protocol_version").GetUInt32() != ProtocolConstants.Version ||
                !root.TryGetProperty("event_id", out _) ||
                !root.TryGetProperty("type", out _))
            {
                throw new ProtocolException("invalid_event", "Agent sent an invalid event envelope");
            }

            return root.Clone();
        }
        catch (OperationCanceledException)
        {
            // A cancelled frame read may already have consumed part of its
            // header or payload, so the event stream cannot be resumed safely.
            _disposed = true;
            await _pipe.DisposeAsync().ConfigureAwait(false);
            throw;
        }
    }

    private async Task<JsonElement> SendAsync(
        string method,
        Action<Utf8JsonWriter> writeParameters,
        CancellationToken cancellationToken)
    {
        ThrowIfDisposed();
        if (_streaming)
        {
            throw new InvalidOperationException("An event-stream connection cannot send more requests");
        }

        await _requestGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            string requestId = $"settings-{Interlocked.Increment(ref _requestSequence)}";
            using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            linked.CancelAfter(_responseTimeout);
            try
            {
                await ProtocolFrame.WriteAsync(
                    _pipe,
                    writer =>
                    {
                        writer.WriteStartObject();
                        writer.WriteNumber("protocol_version", ProtocolConstants.Version);
                        writer.WriteString("request_id", requestId);
                        writer.WriteString("method", method);
                        writer.WritePropertyName("params");
                        writeParameters(writer);
                        writer.WriteEndObject();
                    },
                    linked.Token).ConfigureAwait(false);
                using JsonDocument? document = await ProtocolFrame.ReadAsync(_pipe, linked.Token).ConfigureAwait(false);
                if (document is null)
                {
                    throw new EndOfStreamException("InputFlow agent closed the pipe before responding");
                }

                return ParseResponse(document.RootElement, requestId);
            }
            catch (OperationCanceledException)
            {
                // Once a framed request may have reached the server, abandoning its
                // response makes this byte stream unsafe to reuse: the late response
                // could otherwise be mistaken for the next request. Close on both a
                // caller cancellation and the client-side response deadline.
                _disposed = true;
                await _pipe.DisposeAsync().ConfigureAwait(false);
                if (cancellationToken.IsCancellationRequested)
                {
                    throw;
                }

                throw new TimeoutException(
                    $"InputFlow request `{method}` timed out after {_responseTimeout}; a mutating result may be unknown");
            }
        }
        finally
        {
            _requestGate.Release();
        }
    }

    private static JsonElement ParseResponse(JsonElement root, string requestId)
    {
        if (root.GetProperty("protocol_version").GetUInt32() != ProtocolConstants.Version)
        {
            throw new ProtocolException("unsupported_protocol", "Agent responded with a different protocol version");
        }

        if (!string.Equals(root.GetProperty("request_id").GetString(), requestId, StringComparison.Ordinal))
        {
            throw new ProtocolException("request_id_mismatch", "Agent response request_id does not match the request");
        }

        string? type = root.GetProperty("type").GetString();
        if (type == "success")
        {
            return root.GetProperty("result").Clone();
        }

        if (type == "error")
        {
            JsonElement error = root.GetProperty("error");
            throw new ProtocolException(
                error.GetProperty("code").GetString() ?? "unknown_error",
                error.GetProperty("message").GetString() ?? "The agent reported an error",
                error.TryGetProperty("details", out JsonElement details) ? details.GetString() : null);
        }

        throw new ProtocolException("invalid_response", "Agent response has an unknown type");
    }

    private static void WriteEmptyObject(Utf8JsonWriter writer)
    {
        writer.WriteStartObject();
        writer.WriteEndObject();
    }

    private static void WriteConfigParameter(Utf8JsonWriter writer, JsonElement config)
    {
        writer.WriteStartObject();
        writer.WritePropertyName("config");
        config.WriteTo(writer);
        writer.WriteEndObject();
    }

    public async ValueTask DisposeAsync()
    {
        if (_resourcesDisposed)
        {
            return;
        }

        _resourcesDisposed = true;
        _disposed = true;
        await _pipe.DisposeAsync().ConfigureAwait(false);
        _requestGate.Dispose();
    }

    private void ThrowIfDisposed()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
    }

}

public sealed record HandshakeInfo(
    string ServerName,
    string ServerVersion,
    uint ProtocolVersion,
    uint SchemaVersion,
    IReadOnlyList<string> Capabilities,
    int MaximumFrameBytes,
    int MaximumRequestsPerConnection,
    int EventQueueCapacity,
    int ClientResponseTimeoutMilliseconds)
{
    internal static HandshakeInfo Parse(JsonElement value)
    {
        string[] capabilities = value.GetProperty("capabilities")
            .EnumerateArray()
            .Select(item => item.GetString() ?? string.Empty)
            .ToArray();
        return new HandshakeInfo(
            value.GetProperty("server_name").GetString() ?? "inputflow-agent",
            value.GetProperty("server_version").GetString() ?? "unknown",
            value.GetProperty("protocol_version").GetUInt32(),
            value.GetProperty("schema_version").GetUInt32(),
            capabilities,
            value.GetProperty("max_frame_bytes").GetInt32(),
            value.GetProperty("max_requests_per_connection").GetInt32(),
            value.GetProperty("event_queue_capacity").GetInt32(),
            value.GetProperty("client_response_timeout_ms").GetInt32());
    }
}

public sealed class InputFlowEventSubscription : IAsyncDisposable
{
    private readonly InputFlowClient _client;

    internal InputFlowEventSubscription(InputFlowClient client)
    {
        _client = client;
    }

    public ValueTask<JsonElement?> ReadAsync(CancellationToken cancellationToken = default) =>
        _client.ReadEventAsync(cancellationToken);

    public ValueTask DisposeAsync() => _client.DisposeAsync();
}
