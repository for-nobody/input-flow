using System.Diagnostics;

namespace InputFlow.Protocol;

public static class ProtocolConstants
{
    public const uint Version = 1;
    public const int MaximumFrameBytes = 1024 * 1024;
    public const int DefaultResponseTimeoutMilliseconds = 3_000;

    public static string DefaultPipeName => $"InputFlow.Agent.v1.{Process.GetCurrentProcess().SessionId}";
}
