namespace InputFlow.Settings.Core;

public sealed class EventSequenceTracker
{
    private ulong _lastEventId;

    public void Reset() => _lastEventId = 0;

    /// <summary>Returns true when the observed id proves one or more events were dropped.</summary>
    public bool Observe(ulong eventId)
    {
        bool gap = _lastEventId != 0 && eventId != _lastEventId + 1;
        _lastEventId = eventId;
        return gap;
    }
}
