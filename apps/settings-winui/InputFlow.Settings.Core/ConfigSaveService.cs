using System.Text.Json;

namespace InputFlow.Settings.Core;

public enum SaveResultKind
{
    Applied,
    ExternalChangeDetected,
    ValidationFailed,
    PersistenceFailed,
    RuntimeCancelled,
    RuntimeFailed,
    RuntimeBusy,
    OutcomeUnknown,
    RolledBackAfterTimeout,
    RecoveryRequired,
}

public sealed record SaveResult(
    SaveResultKind Kind,
    string Message,
    ConfigDocument FrozenDraft,
    ConfigDocument? ConfirmedFormal,
    IReadOnlyList<string> ValidationErrors,
    IReadOnlyList<string> CleanupWarnings)
{
    public bool IsApplied => Kind == SaveResultKind.Applied;
}

public sealed class ConfigSaveService
{
    private static readonly TimeSpan ReconciliationDelay = TimeSpan.FromMilliseconds(250);
    private const int ReconciliationAttempts = 8;
    private readonly IAgentGateway _agent;

    public ConfigSaveService(IAgentGateway agent)
    {
        _agent = agent;
    }

    public async Task<SaveResult> SaveAsync(
        ConfigDocument draft,
        ConfigDocument editingBaseline,
        bool overwriteExternalChanges,
        CancellationToken cancellationToken = default)
    {
        ConfigDocument frozen = draft.DeepCopy();
        ConfigDocument current = await _agent.GetConfigAsync(cancellationToken).ConfigureAwait(false);
        if (!overwriteExternalChanges && !ConfigCodec.DeepEquals(current, editingBaseline))
        {
            return Result(
                SaveResultKind.ExternalChangeDetected,
                "正式规则已在其他客户端改变。可重新加载，或明确确认覆盖；该检查不是原子并发事务。",
                frozen);
        }

        JsonElement validation = await _agent.ValidateConfigAsync(frozen, cancellationToken)
            .ConfigureAwait(false);
        if (!validation.GetProperty("valid").GetBoolean())
        {
            return new SaveResult(
                SaveResultKind.ValidationFailed,
                "Agent 拒绝了草稿，请修正规则后再保存。",
                frozen,
                null,
                ReadStrings(validation.GetProperty("errors")),
                Array.Empty<string>());
        }

        JsonElement apply;
        try
        {
            apply = await _agent.ApplyConfigAsync(frozen, cancellationToken).ConfigureAwait(false);
        }
        catch (Exception error) when (MayLeaveMutationOutcomeUnknown(error))
        {
            return await ReconnectAndReconcileAsync(
                frozen,
                editingBaseline,
                $"保存请求未收到最终响应：{error.Message}",
                cancellationToken).ConfigureAwait(false);
        }

        string outcome = apply.GetProperty("outcome").GetString() ?? "unknown";
        IReadOnlyList<string> cleanupWarnings = ReadCleanupWarnings(apply);
        return outcome switch
        {
            "applied" => await VerifyAppliedAsync(frozen, cleanupWarnings, cancellationToken)
                .ConfigureAwait(false),
            "validation_failed" => new SaveResult(
                SaveResultKind.ValidationFailed,
                "Agent 在提交时拒绝了草稿。",
                frozen,
                null,
                ReadStrings(apply.GetProperty("validation_errors")),
                cleanupWarnings),
            "persistence_failed" => Result(
                SaveResultKind.PersistenceFailed,
                ApplyError(apply, "配置无法持久化，旧规则继续运行。"),
                frozen,
                cleanupWarnings),
            "runtime_cancelled" => Result(
                SaveResultKind.RuntimeCancelled,
                ApplyError(apply, "运行时替换尚未开始且已取消，旧规则继续运行。"),
                frozen,
                cleanupWarnings),
            "runtime_failed" => Result(
                apply.GetProperty("recovery_required").GetBoolean()
                    ? SaveResultKind.RecoveryRequired
                    : SaveResultKind.RuntimeFailed,
                ApplyError(apply, "运行时替换失败。"),
                frozen,
                cleanupWarnings),
            "runtime_busy" => Result(
                apply.GetProperty("recovery_required").GetBoolean()
                    ? SaveResultKind.RecoveryRequired
                    : SaveResultKind.RuntimeBusy,
                ApplyError(apply, "Agent 正在核对上一项配置应用，当前草稿没有再次提交。"),
                frozen,
                cleanupWarnings),
            "runtime_outcome_unknown" => await ReconcileAsync(
                frozen,
                editingBaseline,
                ApplyError(apply, "运行时结果待核对。"),
                cleanupWarnings,
                cancellationToken).ConfigureAwait(false),
            _ => Result(
                SaveResultKind.OutcomeUnknown,
                $"Agent 返回未知 apply outcome `{outcome}`，草稿已保留。",
                frozen,
                cleanupWarnings),
        };
    }

    private async Task<SaveResult> VerifyAppliedAsync(
        ConfigDocument frozen,
        IReadOnlyList<string> cleanupWarnings,
        CancellationToken cancellationToken)
    {
        try
        {
            ConfigDocument formal = await _agent.GetConfigAsync(cancellationToken).ConfigureAwait(false);
            AgentStatus status = await _agent.GetStatusAsync(cancellationToken).ConfigureAwait(false);
            if (ConfigCodec.DeepEquals(formal, frozen) && status.RuleCount == frozen.EnabledRuleCount)
            {
                return new SaveResult(
                    SaveResultKind.Applied,
                    cleanupWarnings.Count == 0
                        ? "规则已经由 Agent 验证、保存、应用并读回核对。"
                        : "规则已应用，但清理旧恢复文件时有警告。",
                    frozen,
                    formal,
                    Array.Empty<string>(),
                    cleanupWarnings);
            }

            return Result(
                SaveResultKind.OutcomeUnknown,
                "Agent 报告已应用，但重新读取的配置或启用规则数不一致；草稿已保留，请重连核对。",
                frozen,
                cleanupWarnings);
        }
        catch (Exception error) when (MayLeaveMutationOutcomeUnknown(error))
        {
            return await ReconnectAndReconcileAsync(
                frozen,
                frozen,
                $"Agent 报告已应用，但读回核对失败：{error.Message}",
                cancellationToken).ConfigureAwait(false);
        }
    }

    private async Task<SaveResult> ReconnectAndReconcileAsync(
        ConfigDocument frozen,
        ConfigDocument editingBaseline,
        string reason,
        CancellationToken cancellationToken)
    {
        try
        {
            await _agent.ReconnectAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception reconnectError)
        {
            return Result(
                SaveResultKind.OutcomeUnknown,
                $"{reason}；重连失败：{reconnectError.Message}。请求超时不等于保存失败。",
                frozen);
        }

        return await ReconcileAsync(
            frozen,
            editingBaseline,
            reason,
            Array.Empty<string>(),
            cancellationToken).ConfigureAwait(false);
    }

    private async Task<SaveResult> ReconcileAsync(
        ConfigDocument frozen,
        ConfigDocument editingBaseline,
        string reason,
        IReadOnlyList<string> cleanupWarnings,
        CancellationToken cancellationToken)
    {
        AgentStatus? lastStatus = null;
        ConfigDocument? lastFormal = null;
        for (int attempt = 0; attempt < ReconciliationAttempts; attempt++)
        {
            lastStatus = await _agent.GetStatusAsync(cancellationToken).ConfigureAwait(false);
            lastFormal = await _agent.GetConfigAsync(cancellationToken).ConfigureAwait(false);
            switch (lastStatus.ApplyReconciliation)
            {
                case "pending":
                    await Task.Delay(ReconciliationDelay, cancellationToken).ConfigureAwait(false);
                    continue;
                case "applied_after_timeout":
                    if (ConfigCodec.DeepEquals(lastFormal, frozen) &&
                        lastStatus.RuleCount == frozen.EnabledRuleCount)
                    {
                        return new SaveResult(
                            SaveResultKind.Applied,
                            "先前超时的保存已由 Agent 核对为成功，并已读回正式配置。",
                            frozen,
                            lastFormal,
                            Array.Empty<string>(),
                            cleanupWarnings);
                    }

                    break;
                case "rolled_back_after_timeout":
                    return new SaveResult(
                        SaveResultKind.RolledBackAfterTimeout,
                        "先前超时的运行时替换最终失败；Agent 已恢复旧正式配置，当前草稿仍保留。",
                        frozen,
                        lastFormal,
                        Array.Empty<string>(),
                        cleanupWarnings);
                case "recovery_required":
                    return new SaveResult(
                        SaveResultKind.RecoveryRequired,
                        "Agent 无法确定或恢复运行时结果。请重启 Agent 后重新读取正式配置；不要重复提交。",
                        frozen,
                        lastFormal,
                        Array.Empty<string>(),
                        cleanupWarnings);
                case "settled":
                    if (ConfigCodec.DeepEquals(lastFormal, frozen) &&
                        lastStatus.RuleCount == frozen.EnabledRuleCount)
                    {
                        return new SaveResult(
                            SaveResultKind.Applied,
                            "请求响应丢失，但重连后正式配置与已提交草稿一致。",
                            frozen,
                            lastFormal,
                            Array.Empty<string>(),
                            cleanupWarnings);
                    }

                    if (ConfigCodec.DeepEquals(lastFormal, editingBaseline))
                    {
                        return new SaveResult(
                            SaveResultKind.RolledBackAfterTimeout,
                            "重连后正式配置仍是编辑前快照；草稿未被再次提交。",
                            frozen,
                            lastFormal,
                            Array.Empty<string>(),
                            cleanupWarnings);
                    }

                    break;
            }

            break;
        }

        string state = lastStatus?.ApplyReconciliation ?? "unknown";
        return new SaveResult(
            SaveResultKind.OutcomeUnknown,
            $"{reason}；核对状态为 `{state}`，无法证明已应用或已回滚。草稿已保留，请稍后重连。",
            frozen,
            lastFormal,
            Array.Empty<string>(),
            cleanupWarnings);
    }

    private static bool MayLeaveMutationOutcomeUnknown(Exception error) =>
        error is TimeoutException or OperationCanceledException or EndOfStreamException or IOException;

    private static string ApplyError(JsonElement apply, string fallback)
    {
        JsonElement error = apply.GetProperty("error");
        return error.ValueKind == JsonValueKind.Null ? fallback : error.GetString() ?? fallback;
    }

    private static IReadOnlyList<string> ReadCleanupWarnings(JsonElement apply)
    {
        JsonElement save = apply.GetProperty("save");
        return save.ValueKind == JsonValueKind.Null
            ? Array.Empty<string>()
            : ReadStrings(save.GetProperty("cleanup_warnings"));
    }

    private static IReadOnlyList<string> ReadStrings(JsonElement values) =>
        values.EnumerateArray().Select(value => value.GetString() ?? string.Empty).ToArray();

    private static SaveResult Result(
        SaveResultKind kind,
        string message,
        ConfigDocument frozen,
        IReadOnlyList<string>? cleanupWarnings = null) =>
        new(
            kind,
            message,
            frozen,
            null,
            Array.Empty<string>(),
            cleanupWarnings ?? Array.Empty<string>());
}
