using System.Buffers.Binary;
using System.Text.Json;

namespace InputFlow.Protocol;

public static class ProtocolFrame
{
    public static async ValueTask WriteAsync<T>(
        Stream stream,
        T value,
        CancellationToken cancellationToken = default)
    {
        byte[] payload = JsonSerializer.SerializeToUtf8Bytes(value, ProtocolJson.Options);
        if (payload.Length == 0 || payload.Length > ProtocolConstants.MaximumFrameBytes)
        {
            throw new ProtocolException(
                "message_too_large",
                $"JSON payload length must be between 1 and {ProtocolConstants.MaximumFrameBytes} bytes");
        }

        byte[] header = new byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32LittleEndian(header, (uint)payload.Length);
        await stream.WriteAsync(header, cancellationToken).ConfigureAwait(false);
        await stream.WriteAsync(payload, cancellationToken).ConfigureAwait(false);
        await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
    }

    public static async ValueTask<JsonDocument?> ReadAsync(
        Stream stream,
        CancellationToken cancellationToken = default)
    {
        byte[] header = new byte[sizeof(uint)];
        int headerBytes = await ReadExactlyOrEofAsync(stream, header, cancellationToken).ConfigureAwait(false);
        if (headerBytes == 0)
        {
            return null;
        }

        if (headerBytes != header.Length)
        {
            throw new EndOfStreamException("Named Pipe frame ended inside its length header");
        }

        uint length = BinaryPrimitives.ReadUInt32LittleEndian(header);
        if (length == 0 || length > ProtocolConstants.MaximumFrameBytes)
        {
            throw new ProtocolException(
                "malformed_frame",
                $"Named Pipe frame length {length} is outside the supported range");
        }

        byte[] payload = new byte[length];
        int payloadBytes = await ReadExactlyOrEofAsync(stream, payload, cancellationToken).ConfigureAwait(false);
        if (payloadBytes != payload.Length)
        {
            throw new EndOfStreamException(
                $"Named Pipe frame ended after {payloadBytes} of {payload.Length} payload bytes");
        }

        return JsonDocument.Parse(payload, ProtocolJson.DocumentOptions);
    }

    private static async ValueTask<int> ReadExactlyOrEofAsync(
        Stream stream,
        Memory<byte> buffer,
        CancellationToken cancellationToken)
    {
        int received = 0;
        while (received < buffer.Length)
        {
            int count = await stream.ReadAsync(buffer[received..], cancellationToken).ConfigureAwait(false);
            if (count == 0)
            {
                break;
            }

            received += count;
        }

        return received;
    }
}

internal static class ProtocolJson
{
    public static readonly JsonSerializerOptions Options = new(JsonSerializerDefaults.Web)
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DictionaryKeyPolicy = JsonNamingPolicy.SnakeCaseLower,
        PropertyNameCaseInsensitive = false,
        UnmappedMemberHandling = System.Text.Json.Serialization.JsonUnmappedMemberHandling.Disallow,
    };

    public static readonly JsonDocumentOptions DocumentOptions = new()
    {
        AllowTrailingCommas = false,
        CommentHandling = JsonCommentHandling.Disallow,
        MaxDepth = 64,
    };
}
