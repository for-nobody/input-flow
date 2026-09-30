namespace InputFlow.Settings.Core;

/// <summary>
/// Serializes the UI-side capture intent. Invalidation happens before an agent
/// cancel request, so a terminal event racing with Esc/cancel cannot write an
/// obsolete editor field.
/// </summary>
public sealed class CaptureSessionTracker
{
    private readonly object _gate = new();
    private ulong? _activeSession;
    private bool _beginPending;
    private CaptureTerminal? _bufferedTerminal;
    private string? _invalidatedBeginMessage;

    public ulong? ActiveSession
    {
        get
        {
            lock (_gate)
            {
                return _activeSession;
            }
        }
    }

    public void Begin()
    {
        lock (_gate)
        {
            if (_activeSession is not null || _beginPending)
            {
                throw new InvalidOperationException("已有录制会话正在进行");
            }

            _beginPending = true;
            _bufferedTerminal = null;
            _invalidatedBeginMessage = null;
        }
    }

    public CaptureTerminal? CompleteBegin(ulong sessionId)
    {
        lock (_gate)
        {
            if (!_beginPending && _invalidatedBeginMessage is string invalidated)
            {
                _invalidatedBeginMessage = null;
                return new CaptureTerminal(
                    sessionId,
                    CaptureTerminalKind.EventStreamLost,
                    null,
                    invalidated);
            }

            _beginPending = false;
            _activeSession = sessionId;
            if (_bufferedTerminal?.SessionId != sessionId)
            {
                _bufferedTerminal = null;
                return null;
            }

            CaptureTerminal terminal = _bufferedTerminal;
            _bufferedTerminal = null;
            _activeSession = null;
            return terminal;
        }
    }

    public void FailBegin()
    {
        lock (_gate)
        {
            _beginPending = false;
            _bufferedTerminal = null;
            _invalidatedBeginMessage = null;
        }
    }

    public bool InvalidateForCancel(ulong sessionId)
    {
        lock (_gate)
        {
            if (_activeSession != sessionId)
            {
                return false;
            }

            _activeSession = null;
            _bufferedTerminal = null;
            return true;
        }
    }

    public bool Observe(CaptureTerminal terminal)
    {
        lock (_gate)
        {
            if (_activeSession == terminal.SessionId)
            {
                _activeSession = null;
                return true;
            }

            if (_beginPending)
            {
                _bufferedTerminal = terminal;
            }

            return false;
        }
    }

    public CaptureTerminal? EventStreamLost(string message)
    {
        lock (_gate)
        {
            CaptureTerminal? terminal = _activeSession is ulong sessionId
                ? new CaptureTerminal(
                    sessionId,
                    CaptureTerminalKind.EventStreamLost,
                    null,
                    message)
                : null;
            if (_beginPending)
            {
                _invalidatedBeginMessage = message;
            }
            _activeSession = null;
            _beginPending = false;
            _bufferedTerminal = null;
            return terminal;
        }
    }
}
