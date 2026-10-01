namespace InputFlow.Settings.Core;

public enum CaptureUiIntentState
{
    Idle,
    Starting,
    Active,
    CancelPending,
    Cancelling,
}

public enum CaptureBeginDisposition
{
    Listen,
    CancelImmediately,
}

/// <summary>
/// Owns the UI intent around the interval where begin_capture has been sent but
/// its session id is not known yet. This is deliberately separate from the
/// coordinator's protocol-session tracker: the UI must lock its target field
/// immediately and remember an Esc/cancel intent before it can name a session.
/// </summary>
public sealed class CaptureUiIntentTracker
{
    public CaptureUiIntentState State { get; private set; }

    public ulong? SessionId { get; private set; }

    public bool IsInProgress => State != CaptureUiIntentState.Idle;

    public bool TryBegin()
    {
        if (State != CaptureUiIntentState.Idle)
        {
            return false;
        }

        State = CaptureUiIntentState.Starting;
        SessionId = null;
        return true;
    }

    public CaptureBeginDisposition CompleteBegin(ulong sessionId)
    {
        if (State is not (CaptureUiIntentState.Starting or CaptureUiIntentState.CancelPending))
        {
            throw new InvalidOperationException($"Cannot complete capture begin while state is {State}");
        }

        SessionId = sessionId;
        if (State == CaptureUiIntentState.CancelPending)
        {
            State = CaptureUiIntentState.Cancelling;
            return CaptureBeginDisposition.CancelImmediately;
        }

        State = CaptureUiIntentState.Active;
        return CaptureBeginDisposition.Listen;
    }

    /// <summary>
    /// Records cancellation immediately. A null result means begin is still
    /// pending and the caller must cancel after CompleteBegin supplies the id.
    /// </summary>
    public ulong? RequestCancel()
    {
        switch (State)
        {
            case CaptureUiIntentState.Starting:
                State = CaptureUiIntentState.CancelPending;
                return null;
            case CaptureUiIntentState.Active:
                State = CaptureUiIntentState.Cancelling;
                return SessionId;
            default:
                return null;
        }
    }

    public bool TryAcceptTerminal(ulong sessionId)
    {
        if (State != CaptureUiIntentState.Active || SessionId != sessionId)
        {
            return false;
        }

        Reset();
        return true;
    }

    public bool IsActiveSession(ulong sessionId) =>
        State == CaptureUiIntentState.Active && SessionId == sessionId;

    /// <summary>
    /// Clears a begin that failed before a usable session id arrived and
    /// reports whether the user had already requested cancellation.
    /// </summary>
    public bool FailBegin()
    {
        bool cancellationRequested = State == CaptureUiIntentState.CancelPending;
        Reset();
        return cancellationRequested;
    }

    public void CompleteCancellation(ulong sessionId)
    {
        if (State == CaptureUiIntentState.Cancelling && SessionId == sessionId)
        {
            Reset();
        }
    }

    private void Reset()
    {
        State = CaptureUiIntentState.Idle;
        SessionId = null;
    }
}
