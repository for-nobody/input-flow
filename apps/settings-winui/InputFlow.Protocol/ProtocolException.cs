namespace InputFlow.Protocol;

public sealed class ProtocolException : Exception
{
    public ProtocolException(string code, string message, string? details = null)
        : base(details is null ? message : $"{message}: {details}")
    {
        Code = code;
        Details = details;
    }

    public string Code { get; }

    public string? Details { get; }
}
