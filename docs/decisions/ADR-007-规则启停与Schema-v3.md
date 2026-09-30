# ADR-007：持久化规则启停与 Schema v3

- 状态：已接受（Accepted）
- 日期：2026-09-30
- 涉及模块：`inputflow-config`、`inputflow-runtime`、`inputflow-protocol`、`InputFlow.Settings.Core`
- 后续编号：原规划中的鼠标方向 ADR 顺延为 ADR-008

## 背景

Phase E 要求单条规则可启用/禁用，但 Schema v2 的规则只有 `id/trigger/action`。删除规则、仅保存在 UI 内存中的开关或全局 pause 都不具备单规则禁用语义。agent 又必须继续作为唯一正式配置和运行时索引所有者。

## 决策

1. 当前正式配置升级为 Schema v3，每条规则新增必需布尔字段 `enabled`。
2. Schema v1 字符串键和 Schema v2 显式键文档继续可读；迁移到内存 v3 时所有规则确定为 `enabled: true`。读取迁移不直接覆盖原文件；下一次成功保存写 v3，既有 committed backup 继续保留回滚来源。
3. 所有规则（包括禁用规则）都必须有非空且全配置唯一的 ID，并且 trigger/action/键身份/timeout 必须合法。禁用不能用来保存损坏或未知结构。
4. 只有 `enabled: true` 的规则参与紧急旁路键可达性检查、运行时冲突检测和 `RuleIndex` 编译。禁用规则之间、禁用与启用规则之间允许 trigger 冲突；重新启用时，整份草稿必须重新通过 agent 校验。
5. runtime、status、validate/apply 返回的 `rule_count` 明确定义为启用规则数。禁用规则仍由 `get_config` 完整返回并保持原顺序。
6. 切换启停仍走完整 apply 事务。Hook owner 的既有 `replace_rules` 会先冲刷 pending，并保留已经消费的 release tombstone；Schema 迁移不引入第二条替换路径。

## 协议兼容

Named Pipe wire 版本保持 v1。v1 的 `ConfigParams/ConfigResult` 本来就承载版本化配置文档，handshake 也独立报告 `schema_version`，因此 Schema v3 可以明确表达而无需改变 frame/envelope/method 语义。agent capability 从 `config_v2` 更新为 `config_v3`；旧 schema-v2 agent 会通过 handshake 被 Phase E UI 明确拒绝，不会被静默误读。

共享 fixture 增加 `fixtures/config/v3-valid.json`，协议 v1 的 `get-config-response` 更新为内嵌 v3。Rust 同时验证 v1/v2/v3 迁移，C# 强类型配置边界验证三代 fixture 并只输出 v3。

## 失败模型与不变量

- 重复 ID 即使分处启用/禁用规则也拒绝，避免 UI 操作和诊断引用歧义。
- 禁用规则不暂扣输入、不计入 `rule_count`；重新启用前不会暗中改变 matcher。
- apply 失败、超时或 reconciliation 未决时，UI 开关只是脏草稿，不能显示为正式生效。
- v1/v2 backup 的恢复仍走同一个 loader；恢复成功后在内存中得到 v3，并给出兼容提示。

## 后果

Schema v3 解决了 Phase E 的持久化启停缺口，同时不扩展 trigger/action 能力。代价是 Rust/C# fixtures、handshake schema version、文档和所有构造 `RuleConfig` 的测试都必须显式处理 `enabled`；这由跨语言 contract 和迁移测试约束。
