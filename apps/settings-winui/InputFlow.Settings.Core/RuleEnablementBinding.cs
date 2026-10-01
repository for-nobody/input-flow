namespace InputFlow.Settings.Core;

/// <summary>
/// Separates a rule row's initial binding value from a later user-originated
/// value change. Constructing or re-reading the current value never writes
/// back to the draft; a genuinely different two-way binding value does.
/// </summary>
public sealed class RuleEnablementBinding
{
    private readonly Action<string, bool> _changed;
    private bool _enabled;

    public RuleEnablementBinding(string ruleId, bool enabled, Action<string, bool> changed)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ruleId);
        ArgumentNullException.ThrowIfNull(changed);
        RuleId = ruleId;
        _enabled = enabled;
        _changed = changed;
    }

    public string RuleId { get; }

    public bool Enabled
    {
        get => _enabled;
        set
        {
            if (_enabled == value)
            {
                return;
            }

            _enabled = value;
            _changed(RuleId, value);
        }
    }
}
