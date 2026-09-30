namespace InputFlow.Settings.Core;

public sealed class DraftSession
{
    private ConfigDocument? _formalSnapshot;
    private ConfigDocument? _draft;

    public event EventHandler? Changed;

    public bool IsLoaded => _formalSnapshot is not null && _draft is not null;

    public bool IsDirty => IsLoaded && !ConfigCodec.DeepEquals(_formalSnapshot!, _draft!);

    public ConfigDocument FormalSnapshot => _formalSnapshot?.DeepCopy()
        ?? throw new InvalidOperationException("No formal configuration has been loaded");

    public ConfigDocument Draft => _draft?.DeepCopy()
        ?? throw new InvalidOperationException("No draft configuration has been loaded");

    public void Load(ConfigDocument formal)
    {
        ArgumentNullException.ThrowIfNull(formal);
        _formalSnapshot = formal.DeepCopy();
        _draft = formal.DeepCopy();
        Changed?.Invoke(this, EventArgs.Empty);
    }

    public void AcceptSaved(ConfigDocument formal)
    {
        Load(formal);
    }

    public void ReplaceDraft(ConfigDocument draft)
    {
        EnsureLoaded();
        _draft = draft.DeepCopy();
        Changed?.Invoke(this, EventArgs.Empty);
    }

    public void UpsertRule(RuleDocument rule, string? originalId = null)
    {
        EnsureLoaded();
        ArgumentNullException.ThrowIfNull(rule);
        int index = originalId is null
            ? -1
            : _draft!.Rules.FindIndex(candidate => candidate.Id == originalId);
        if (index >= 0)
        {
            _draft!.Rules[index] = rule.DeepCopy();
        }
        else
        {
            _draft!.Rules.Add(rule.DeepCopy());
        }

        Changed?.Invoke(this, EventArgs.Empty);
    }

    public bool RemoveRule(string id)
    {
        EnsureLoaded();
        int removed = _draft!.Rules.RemoveAll(rule => rule.Id == id);
        if (removed > 0)
        {
            Changed?.Invoke(this, EventArgs.Empty);
        }

        return removed > 0;
    }

    public bool SetRuleEnabled(string id, bool enabled)
    {
        EnsureLoaded();
        int index = _draft!.Rules.FindIndex(rule => rule.Id == id);
        if (index < 0 || _draft.Rules[index].Enabled == enabled)
        {
            return false;
        }

        _draft.Rules[index] = _draft.Rules[index] with { Enabled = enabled };
        Changed?.Invoke(this, EventArgs.Empty);
        return true;
    }

    public void SetEmergencyKey(KeyIdentity key)
    {
        EnsureLoaded();
        if (key.Mode != KeyMatchMode.Logical)
        {
            throw new ArgumentException("The emergency bypass key must be logical", nameof(key));
        }

        _draft = _draft! with { EmergencyBypassKey = key };
        Changed?.Invoke(this, EventArgs.Empty);
    }

    private void EnsureLoaded()
    {
        if (!IsLoaded)
        {
            throw new InvalidOperationException("No configuration draft has been loaded");
        }
    }
}
