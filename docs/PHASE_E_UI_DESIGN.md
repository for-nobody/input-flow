# M7 Phase E WinUI 3 设置程序设计

- 状态：实施前设计基线
- 日期：2026-09-30
- 依据：ADR-004～007、ADR-005 的键身份、ADR-006 的 Named Pipe 失败语义，以及 `tag_5.1` Phase E 任务

## 1. 信息架构与状态草图

设置程序采用紧凑的 `NavigationView`，顶层入口只有“快捷规则”“设置”，页脚为“关于”。所有运行结果来自 agent；UI 不安装 Hook、不直接写 `config.json`。

### 快捷规则

- 顶部状态条显示连接、agent `phase`、暂停状态、启用规则数、最近错误和 apply reconciliation。`ready` 只写“Agent 已就绪”，不推断 Hook 永远健康。
- `CommandBar` 提供新建、验证并保存、重新加载和暂停/恢复。保存只提交当前草稿。
- 规则视图可选“全部规则”，并按 trigger 第一个键自动生成非空分组。Ctrl/Shift 组内仍展示 Left/Right 稳定 identity；physical 规则按 scan identity 分组；不生成 Fn 或“任意 Ctrl”语义。
- 编辑器只呈现四种 trigger 和一种 `key_chord` action。键字段既可从完整稳定 identity 列表选择，也可逐字段录制；physical 模式只有非零 scan 时可用。

### 设置

- 编辑紧急旁路逻辑键；修改只进入同一配置草稿，最终由 agent 验证冲突。
- 显示实际 agent 状态、暂停控制、最近错误和手动刷新诊断。无 latency 样本时显示“暂无样本”，不显示伪造的 0。
- 配置恢复当前没有安全 IPC，因此只显示 agent 的真实恢复/错误提示和重启指导，不提供假恢复按钮。
- 登录启动、系统辅助功能、PowerToys、Fn、三键和打开程序/网页不作为可操作入口。

### 关于

- 版本来自设置程序集元数据；显示当前进程架构、x64 支持边界、所需 .NET Desktop Runtime / Windows App Runtime，以及当前规则能力范围。

### 窄窗口与辅助功能

- 宽布局为规则列表 + 编辑器；窄布局编辑时将编辑器移到单列并隐藏列表。所有操作保留键盘焦点顺序、可见焦点和 `AutomationProperties.Name`。
- 使用主题资源而非固定前景/背景色，支持浅色、深色和高对比度；内容可滚动，文本允许换行。

### 关键状态呈现

| 状态 | 呈现 | 可恢复动作 |
|---|---|---|
| 离线/ACL/版本不匹配 | 错误 `InfoBar`，不显示已保存 | 重试连接；检查同一用户会话、agent 和版本 |
| 录制中 | 字段目标、session、剩余秒数；其他录制入口禁用 | Esc/取消按钮；断线后重新录制 |
| 校验失败 | 逐条 agent 错误，草稿保留 | 回到对应规则修正，再手动保存 |
| 外部配置已变化 | 覆盖确认对话框并说明仍非 CAS | 重新加载，或明确确认覆盖 |
| apply 成功 | 重读 config/status 核对后才显示成功 | cleanup warning 单独显示 |
| 请求超时/断线 | “结果待核对”，不声称失败且不自动重试 | 重连，读取 reconciliation/config/status |
| recovery required | 持续错误提示，禁用再次保存 | 重启 agent 后重新读取正式配置 |

## 2. 状态所有权

| 状态 | 所有者 | UI 中的副本/规则 |
|---|---|---|
| Hook、暂停、phase、统计、last error | Rust agent | 只读快照；由确认响应或事件后重读更新 |
| 正式配置 | Rust agent | `get_config` 快照；UI 不直接落盘 |
| 配置草稿 | 应用级 draft session | 深拷贝；新增/编辑/删除/启停/紧急键修改使其变脏 |
| 权威校验与冲突 | Rust agent | UI 只做必填/控件范围提示，不复制 `RuleIndex` |
| control pipe | 应用级连接协调层 | 页面不持有 client；超时后的连接作废 |
| event pipe | 应用级连接协调层 | 独立只读连接；event ID 缺口触发 config/status 重同步 |
| apply 状态机 | 保存服务 | 冻结草稿、外部变化检查、validate/apply/reconcile |
| capture session | 连接协调层 + 当前编辑字段 | session ID 严格匹配；取消后旧终态永不写入草稿 |

窗口关闭会取消 UI lifetime token、尽力取消当前 capture、释放 control/event pipe。agent 生命周期不依赖这些对象。

## 3. 关键事件序列

### 启动、事件与重连

1. control pipe 连接并 handshake；核对 wire v1 与 schema v3。
2. 读取 status/config，建立正式快照和未修改草稿。
3. 第二条连接独立 handshake + `subscribe_events`。
4. `status_changed` / `config_applied` 后重读权威状态；heartbeat 只用于存活，不制造状态。
5. event ID 跳变、事件 pipe EOF 或 agent 重启时废弃旧订阅 generation；新连接成功后重读 status/config，旧 generation 回调不得覆盖新视图。

### 保存

1. 深拷贝并冻结当前草稿，提交期间禁用重复保存。
2. 再次 `get_config`，与开始编辑时的正式快照比较。不同则要求重新加载或明确覆盖；提示不是原子 CAS。
3. `validate_config`；失败只展示问题并保留草稿。
4. `apply_config` 一次；任何 timeout/cancel/EOF 都不重试 mutation。
5. 解析全部 `ApplyResult` outcome。`applied` 后仍重读 config/status；内容和启用规则数核对一致才提交本地正式快照。
6. `runtime_outcome_unknown` 或客户端未收到结果时标记待核对，重连后有界读取 reconciliation/config/status。`pending` 等待；`applied_after_timeout`、`rolled_back_after_timeout`、`recovery_required` 分别给出已应用、已回滚和需重启提示。

### Capture

1. 用户明确点击某个键/按钮字段的“录制”；协调层先确认 event subscription 可用。
2. control pipe 调用 `begin_capture(10000)`，保存返回 session ID；一次只等待一个 input down。
3. 只接受同一 session ID 的 `capture_completed`。键字段拒绝鼠标结果，按钮字段拒绝键结果。
4. Esc 始终作为取消意图，不写入字段；要配置 Escape 使用选择器。取消后立即使 session 失效，再尽力调用 `cancel_capture`，因此即使 Hook 先观察到 Esc，迟到结果也会被丢弃。
5. timeout、owner 断线、订阅丢失、agent shutdown 或窗口关闭都会结束等待；不能确定结果时要求重新录制。

## 4. 验收分层

- 代码可证：Schema v1/v2→v3、禁用规则不进入索引、启用规则数、草稿隔离、保存状态机、事件缺口、capture 旧 session 过滤。
- 自动故障注入：校验失败、外部变化、request timeout 后 applied/rolled-back/recovery reconciliation、订阅断线。
- Windows 联合观察：真实 agent 的编辑/保存/读回，托盘/F12/UI 暂停同步，设置单实例，关闭 UI 后 agent 存活，键盘/高对比度/缩放/UIA。
- 未执行项继续单列：M6 右键菜单/墓碑、repeat、UIPI、100k/高负载、鼠标位置/关闭边界，以及 Explorer 真重启；构建或 UI 演示不替代这些证据。
