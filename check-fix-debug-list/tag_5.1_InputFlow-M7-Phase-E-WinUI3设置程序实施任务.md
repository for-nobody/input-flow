# InputFlow M7 Phase E：WinUI 3 设置程序实施任务

> 用途：把本文件放入项目的 `check-fix-debug-list/` 后交给 VS Code 中的 Codex 执行。本文取代 `tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md` 中的 **Phase E** 细则；Phase A–D 的既有决策、M6 可靠性约束和 Phase F/M8 的边界继续有效。
>
> 编写依据：用户提供的 2026-09-30 压缩包；归档 Git HEAD 为 `03a9fd073f035244e9d13ce3b6add7d26f62949e`（`feat: complete M7 Phase C hardening and Phase D IPC`）。归档中的文件与 HEAD 除行尾转换外没有语义差异。**实际开工时重新核对真实仓库的 HEAD、分支、`AGENTS.md` 和未提交修改；不得用归档基线覆盖用户的新工作。**
>
> 交付目标：通过 WinUI 3 页面完成**当前已支持规则**的查看、创建、编辑、删除、验证、保存和运行控制；在 UI 退出后 agent 独立运行。所有实际状态与结果来自 Phase D 协议，不使用演示数据冒充运行结果。

## 1. 开工核对与事实基线

先读 `README.md`、`Steps.md`、`docs/PROJECT_PLAN.md`、`docs/BUILD_WINDOWS.md`、`docs/decisions/ADR-004～006`、`check-fix-debug-list/tag_5_*任务.md` 与 `tag_5_*记录.md`，再检查实际源码和测试：

- `apps/settings-winui/InputFlow.Settings/`：当前只有可构建、可启动和退出的 Phase A 原生窗口；`MainPage.xaml` 仍是 smoke 提示。
- `apps/settings-winui/InputFlow.Protocol/`：已实现 `InputFlowClient.ConnectAsync`、status/config/validate/apply/pause/resume/stats/capture 和独立事件订阅。客户端返回 `JsonElement`，默认请求 deadline 3 秒；超时/取消后连接不可重用，**mutating 请求的结果可能未知**。
- `apps/settings-winui/InputFlow.Protocol.ContractTests/` 与 `fixtures/protocol/v1/`：已有跨语言 golden 契约和真实 agent live contract。协议为 v1；事件订阅占用单独的只读连接。
- `crates/inputflow-config/src/config.rs`：当前正式 Schema v2 仅有 `schema_version`、`emergency_bypass_key`、`rules`；规则只有 `id/trigger/action`，**没有持久化的单规则 enabled 字段**。
- 当前 trigger：`key_chord`、`key_mouse_button`、`hold`、`hold_mouse_button`；当前 action：`key_chord`。鼠标按钮有 Left/Right/Middle/XButton1/XButton2。capture 一次只返回一个键或鼠标按钮的 down；不是整个组合的录制。
- Phase D 执行记录为 143/143 Rust 测试、C# Debug/Release 构建和 live contract 通过；这是归档的历史证据，开工时以重新运行结果为准。Phase C 的 F12/托盘同步及 apply 超时 reconciliation 已修复；不得恢复旧的双重状态或超时后盲目重试。

本阶段不引入 Tauri、WebView、Node、另一个 Hook、另一个规则校验器或第二个正式配置写入方。UI 可以维护**草稿**，正式配置仍由 agent 验证、保存和应用。

## 2. 先交付可审阅的设计，再分段实现

在修改页面前，用简短文档或本任务的执行记录确认三件事：

1. 两个顶层入口“**快捷规则**”和“**设置**”的信息架构，以及页面窄窗口、离线、录制中、校验失败、保存结果未知时的状态草图。采用紧凑的原生 Windows 工具风格，评估 `NavigationView`、`CommandBar`、`InfoBar`、`ContentDialog`、`ToggleSwitch`、`NumberBox`；不需要空洞仪表盘或大规模视觉重设计。
2. UI 状态模型：agent 状态、连接状态、正式配置快照、未保存草稿、校验结果、apply reconciliation、capture session 与事件订阅分别由谁拥有，哪些变化会使草稿变脏。不得让页面控件直接管理底层 pipe 生命周期。
3. 每种失败的恢复动作与呈现文字，特别是“请求超时 ≠ 保存失败”。先列关键事件序列和验收办法，再写页面代码。

按下列 E0–E4 的小段推进；每段留下可复查记录，不一次生成整套 UI 后才考虑协议与错误语义。

### E0. 处理规则启停的持久化缺口

原 Phase E 要求“启用／禁用单条规则”，但当前 Schema v2 没有 enabled 字段。**不得把删除规则、仅保存在 UI 内存中的布尔值、或全局 pause 冒充单规则禁用。**

推荐在独立提交中设计 Schema v3：为规则添加持久化 `enabled`，v1/v2 读取时默认迁移为 true；禁用规则保留完整 id/trigger/action 而不进入运行时索引。明确重复 ID、禁用规则与启用规则的冲突策略，令 `rule_count` 明确表示启用的规则数；保持 v1/v2 兼容读取和 backup 回滚。写迁移/验证/保存/运行时替换的测试，尤其验证启停时 pending 回放和已消费 release tombstone 不回退；更新 ADR、golden fixtures、协议 handshake 的 schema version 以及 C# 配置契约。按协议演进策略判断 wire v1 是否仍可表达新文档，不能悄悄改变已发布契约。如果该迁移无法在 E 中安全完成，先交付创建/编辑/删除且**不展示单规则开关**，在执行记录明确 Phase E 未满足该项验收。

不要趁 Schema 迁移加入三键/任意长度序列、鼠标方向、按应用生效、打开程序/文件夹/网页等新规则或动作。

### E1. 接入 agent 的状态和连接生命周期

- 在设置程序建立一个可释放的应用级连接协调层：控制连接与只读订阅连接各自 handshake；窗口关闭时取消异步操作、处置连接并退出进程。二次启动应激活现有设置窗口，不能留多个空窗口/多个设置进程；agent 单实例机制不等于 settings 单实例。
- 启动、重连、订阅断线、agent 停止/重启、协议版本不匹配、ACL/权限错误均有真实状态提示。离线页面显示重试与诊断指引；不自动启动另一套 Hook，不声称规则已经保存或 agent 在线。
- 显示 agent 实际 `phase`、`suspended`、`rule_count`、`last_error` 和 `apply_reconciliation`；只显示协议能证实的信息。`phase=ready` 不等于已证明 Hook 永远健康，不显示虚构的“Hook 健康”标志。
- pause/resume 只在得到确认后更新控制结果；F12、托盘和另一 UI 客户端引起的状态变化通过订阅事件同步，并在重连或 event ID 缺口后调用 `get_status` / `get_config` 重同步。heartbeat 不算新状态；不要每秒无条件轮询 stats。
- 事件分发到 WinUI UI 线程；窗口卸载和重连取消旧订阅，避免重复事件、悬空回调或旧 session 的响应覆盖新视图。

### E2. 快捷规则页与编辑器

**布局：**展示“全部规则”与按**触发条件的第一个键**自动生成的分组（Ctrl、Shift、Caps Lock、空格、字母、符号、数字、小键盘等只在有规则时出现）。分组是视图索引，不是新增的“领头键模式”。Ctrl 可在组内区分左/右；当前 Schema 没有“任意 Ctrl”规则语义，不能在 UI 中把它当成已支持的配置。Fn 只有可靠捕获和身份模型后才列为可用分组。

**现有规则的完整往返：**

- 从 `get_config` 取得正式快照并创建本地草稿；新增、编辑、删除、（E0 完成后）启停、取消编辑均不立即落盘。保留原始规则的 id、键身份、顺序和未编辑字段；提交前在 UI 中展示影响范围和未保存状态。
- 编辑器明确区分四种现有 trigger 与一种 `key_chord` action。用可录制键、可选择键、鼠标按钮及有界 `timeout_ms` 控件组合现有规则；不展示假装可保存的三键、打开网页或窗口操作选项。复制/粘贴可通过发送 Ctrl+C/Ctrl+V 等键盘动作表达，不能声称程序已实现专用的剪贴板动作。
- 键身份默认 logical，advanced 可选 physical scan+extended（只有事件带有效非零 scan 时可选）。显示友好键名和稳定 identity；OEM 的布局字符标签仅供预览，按 ADR-005 在非 Hook 路径查询并为死键/查询失败提供稳定 `Oem*` 后备；绝不把本地化字符或显示文本保存为 identity。主 Enter/小键盘 Enter、左右 Ctrl/Alt 与锁定键要能区分。
- 对紧急旁路键做不可达规则提示；保留用户能发现并使用紧急旁路的路径。输入录制只观察，不会替用户阻止 Caps Lock 等键对 Windows 的原本影响；给锁定键提供手动选择入口。
- 编辑器内部可给出可解释的操作提示，但最终冲突、schema、时序合法性以 agent 的 `validate_config` 返回为准，不在 C# 实现第二套 RuleIndex/冲突算法。验证失败时指出具体规则与原因，保留草稿。
- 至少能由 UI 新建并保存一个 `key_chord`、一个 `key_mouse_button`、一个 `hold` 或 `hold_mouse_button`，再读回并正确显示；对不支持的新动作只列为未来计划。

**capture 生命周期：**

- 显式点击“录制”，先确保只读订阅连接可用，再用**控制连接**调用 `begin_capture`（协议范围 100–30000 ms）；根据返回的 session ID 过滤 `capture_completed`，给出倒计时、超时与取消反馈。一次 capture 只录一个输入，组合编辑按字段分别录制；不要误把单次结果当完整组合。
- Esc 和取消按钮要结束 UI 录制且不得把用于取消的 Esc 意外写入草稿。考虑 Hook 先观察到 Esc 再到达 UI 按键事件的竞态：取消意图后忽略旧 session 结果，核对 session ID；若需提供“录制 Esc”，改用选择器或显式模式。
- UI 关闭、控制 pipe 断开、timeout、agent 停止、事件订阅丢失时，停止等待并显示准确终态。尽力调用 `cancel_capture(session_id)`，随后释放控制连接；agent 对 owner 断线有取消保障。事件流丢失导致无法确定录制结果时重新录制，不能猜测或应用上一会话结果。

### E3. 验证、保存与多客户端一致性

- “保存”按次序执行：冻结/复制当前草稿 → agent `validate_config` → 给用户显示验证问题 → agent `apply_config` → 解析结构化 `ApplyResult` → 重新 `get_config` / `get_status` 核对。仅在 `outcome=applied` 且状态可核对时显示成功；展示已提交后的 cleanup warning。
- 区分 `validation_failed`、`persistence_failed`、`runtime_cancelled`、`runtime_failed`、`runtime_busy`、`runtime_outcome_unknown` 和 `recovery_required`。提交期间阻止重复点击；提交或取消后也不能自动重试 mutation。
- 客户端超时、断线或 `runtime_outcome_unknown` 时，保留草稿并显示“结果待核对”，重连后读取 `apply_reconciliation` 与正式配置；`pending` 时等待有界重查询/事件，`applied_after_timeout`、`rolled_back_after_timeout`、`recovery_required` 分别显示结果或人工恢复提示。不得因本地取消就断言服务端没有保存。
- 多个设置客户端可同时修改草稿；Phase D 没有原子 compare-and-swap 配置版本。保存前再次读正式配置，与编辑时快照比较并提示“规则已在其他地方改变”；用户须选择重新加载或确认覆盖。说明读后到 apply 仍可能发生竞态，不能把此提示宣称为强并发事务。
- UI 崩溃或被强制结束后，agent 继续运行，已生效规则不由 UI 内存决定；脏草稿的退出提示不能暗示它已经保存。

### E4. 设置、诊断、窗口与交付

- “设置”先提供当前可支持的内容：紧急旁路键的读取/修改及规则冲突验证、agent 状态/暂停、错误和统计。诊断中的 `callback_latency_us`、`hold_delay_us` 可能无样本，应显示“暂无样本”而不是 0；显示真实输出失败、丢弃计数。配置恢复若没有现成安全 API，仅展示告警/指导入口，不能放一个会假装恢复成功的按钮。
- 登录启动、系统辅助功能开关、PowerToys 双 Ctrl 定位鼠标等仍是未来功能；可在设计稿中注明，正式界面只显示已经接线的入口。Windows 单按 Ctrl 定位鼠标与 PowerToys 双按 Ctrl 是不同来源，不能混称为 InputFlow 开关。Win+W 等系统组合、Fn、任意三键与打开程序/文件夹/网页不进入 Phase E 的规则能力。
- 提供简洁“关于”页，说明当前版本、x64/运行库与支持范围；实际版本读取要有可信来源。支持窗口缩放、浅/深色与高对比度、键盘操作、焦点顺序、屏幕阅读器名称和错误提示；按钮忙碌状态和关闭窗口时释放资源要可靠。
- 关闭最后一个设置窗口后 `InputFlow.Settings.exe` 退出，Rust agent、Hook 和托盘持续工作，agent 不加载 .NET/WinUI/WebView。托盘再次打开设置时激活已有窗口。

## 3. 实现纪律

1. 复用 Phase D 的 `InputFlow.Protocol`；若需要 C# 强类型 DTO/ViewModel，请在 UI 边界进行并继续以 Rust wire DTO 和共享 fixtures 为契约。检查 `JsonElement` 的生命周期，保留必要的 `Clone()`；配置 round-trip 不得丢字段。必要的协议扩展先写 ADR/fixtures/Rust+C# contract，再接 UI。
2. 配置数据、UI 偏好和系统设置分清所有权。UI 不直接写正式 `config.json`；agent 拥有正式保存与 Hook。不要绕开 `validate_config`、`apply_config`、捕获所有权或 IPC ACL。
3. 不在 Hook callback 中调用 UI、Named Pipe、文件打开或磁盘操作；不引入无界逐输入日志、旧 worker/ack 或跨线程直接更改 matcher。M6 的 FIFO 回放、消耗释放墓碑、失败旁路、F12 和两秒退出上限继续有效。
4. 每个小段记录基线、设计选择、修改文件、自动结果、Windows 真实观察及未执行项。遇到现有协议缺少必需能力时明确提出窄范围变更并做跨语言测试；不能以静态文字或假返回值补 UI。

## 4. 验证与完成门槛

自动化只针对实质风险：Schema 迁移与禁用规则、v1/v2/v3 fixtures 往返（若实施 E0）；草稿编辑不触碰正式配置；保存状态机的成功、验证失败、超时结果未知及 reconciliation；事件 ID 缺口和断线重连；capture session/取消竞态与旧事件过滤。继续执行：

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p probe-cli
cargo build -p inputflow-agent --release
dotnet restore .\apps\settings-winui\InputFlow.Settings.slnx
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Debug --no-restore
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Release --no-restore
dotnet run --project .\apps\settings-winui\InputFlow.Protocol.ContractTests\InputFlow.Protocol.ContractTests.csproj -c Debug --no-build
```

Windows 联合验收请使用用户可恢复的配置副本和已知规则，并把“代码可证”“自动故障注入”“真实 Windows 手动观察”“未执行”分栏记录：

| 场景 | 必须观察到的结果 |
|---|---|
| 正常编辑 | 不手改 JSON，创建/编辑/删除代表规则，验证、保存、重启 UI 和 agent 后读回一致；启停若在 E0 落地则重启后也保持。 |
| 实际输入 | 录制/选择 Caps Lock、OEM、方向键、主 Enter/小键盘 Enter、媒体键；硬件缺少某键时记录未执行，用选择器验证构造与 round-trip，不能称为实机输入通过。 |
| 暂停/恢复 | UI、托盘、F12 之间的状态一致，组合失败回放、Caps 指示灯及真实目标窗口结果不因 UI 接入而回退。 |
| 故障 | agent 离线/重启、控制/事件 pipe 断开、设置强杀、保存失败、请求超时、版本不匹配、capture 取消/超时、其他客户端先改配置，都显示准确结果且 agent 保持可控。 |
| 生命周期与资源 | 重复打开/关闭设置无遗留进程、订阅、线程或句柄持续增长；关闭 UI 后 agent 的 CPU/内存/线程/句柄与 Phase C/D 基线对比。 |
| 可访问性 | 键盘全流程、可见焦点、高对比度、缩放、AutomationProperties/UIA 对状态与错误可辨认。 |

M6 尚未补齐的真实右键菜单/释放墓碑、repeat、UIPI、100k/高负载、鼠标位置与关闭边界，以及 Explorer 真正重启的托盘恢复，继续列为独立门槛；不能用 Phase E 构建通过、无输入 smoke 或录制 UI 演示代替。

最终更新 `README.md`、`Steps.md`、`docs/PROJECT_PLAN.md`、`docs/BUILD_WINDOWS.md`、`docs/research-log.md`、相关 ADR/fixtures 和 `tag_5` 执行记录：准确写明 E0–E4 哪些完成、测试与实机证据、剩余限制及下一阶段最小任务。若某门槛未过，保留“Phase E 未完成”状态。
