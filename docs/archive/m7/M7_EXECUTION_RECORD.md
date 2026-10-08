# InputFlow M7/M8：WinUI 3 架构与输入扩展执行记录

> [!CAUTION]
> M7 已完成的历史执行记录。本文中的 M8／下一步文字已失效；当前状态见
> [`../../status/CURRENT_STATUS.md`](../../status/CURRENT_STATUS.md)。

> 日期：2026-09-30～2026-10-02
> 对应任务：`tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md`
> 本次累计范围：Phase A–E 已完成；Phase E 的 E0–E4、真实物理 capture/规则流程、辅助功能、长稳态资源及 M6-01～M6-07 Windows 矩阵均已验收，硬件/环境限制单列保留；Phase F/M8 未执行

## 1. 基线核对

| 项目 | 结果 |
|---|---|
| Git 分支 | `main` |
| 开工 HEAD | `2ead0a9677d2eb6baa2377d4c876ee6a7d383b90`（`2ead0a9 update again with new develop path of this project`） |
| Phase C 开工 HEAD | `e8a4871e5adb829d3f64ce2d8ee366e652c6e5db`（`feat: complete M7 native shell and keyboard identity v2`），与 `origin/main` 同步，工作区干净 |
| Phase E 开工 HEAD | `03a9fd073f035244e9d13ce3b6add7d26f62949e`，分支 `main` 与 `origin/main` 同步；用户提供的未跟踪 `tag_5.1` 任务文件作为本阶段输入保留，未覆盖 |
| 开工工作区 | 非干净；用户已修改任务文件中的基线 HEAD 描述。该改动被保留且未覆盖。 |
| `AGENTS.md` | 仓库中不存在 |
| 已读范围 | README、两份项目规划、Steps、BUILD_WINDOWS、glossary、research-log、ADR-000～004、M6 基线/修复记录、engine/config/windows/probe-cli 全部当前源码与测试、tag_5 全文 |

开工时静态核对确认 tag_5 的前置观察成立：

- `inputflow-windows` 仍通过进程全局 `OnceLock` 承接 matcher/emergency key；尚无产品级 runtime owner、热替换、capture session、agent 或 IPC。
- `inputflow-config` 仍是 Schema v1 字符串 key；`save() -> Result<(), String>` 不能结构化返回提交后 cleanup warning。
- `Key` 和 Windows keymap 尚缺 lock/navigation/OEM/numpad/media 等完整键盘身份；回放主要按 VK。
- 鼠标 move/wheel 不进入 matcher，`MouseKind` 没有 Move 规则语义。
- M6 的释放墓碑、repeat FIFO、输出结果区分、ready 前 timer、Hook 线程串行 pause/resume、锁外统计排序、backup/temp 恢复策略均存在，后续阶段不得回退这些语义。

## 2. 环境探测与安装

| 项目 | 实际结果 |
|---|---|
| Windows | 注册表产品名 Windows 10 Pro 25H2；内核 10.0.26200.9457；x64 |
| Rust | rustc/cargo 1.98.1，`stable-x86_64-pc-windows-msvc` |
| Windows SDK | 工程使用 10.0.26100.0；机器另有 14393/15063/16299/17134 SDK |
| Visual Studio | Build Tools 2022 17.14.41；Build Tools 2026 18.10.2 |
| VS workload | 只有 Windows SDK/Native Desktop Core 等基础组件；未发现 WinUI workload |
| .NET | 新安装 SDK 10.0.401；MSBuild 18.9.11；Desktop Runtime 10.0.12 x64 |
| WinUI CLI 模板 | 新安装 `Microsoft.WindowsAppSDK.WinUI.CSharp.Templates` 0.0.7-alpha |
| Windows App SDK | 工程锁定 2.5.1 |
| BuildTools NuGet | 工程锁定 10.0.28000.2705 |
| Windows App Runtime | 新安装官方 2.5.1 x64；安装程序退出码 0，签名为 Microsoft Corporation 且 Authenticode 有效 |
| Developer Mode | 当前未启用；HKLM 写入被非管理员会话拒绝。没有绕过 UAC，也没有自动提升。 |

`.NET SDK 10.0.401`、WinUI CLI 模板和 Windows App Runtime 2.5.1 x64 是本轮开发机变更。没有安装驱动、修改系统输入设置或安装 Node/Tauri/WebView 技术栈。

## 3. Phase A 方案比较

| 方案 | 实际证据 | 结论 |
|---|---|---|
| Packaged + framework-dependent | 官方模板 restore/build 成功；启动工具报 Developer Mode 未启用 | 本轮不选；启动未验证，不把它写成代码失败 |
| Unpackaged + self-contained | 构建成功；x64 原生窗口启动并正常关闭，exit code 0 | 可用 fallback；输出更大且运行库更新由应用承担 |
| Unpackaged + framework-dependent | 安装 Windows App Runtime 2.5.1 x64 后，Debug/Release build 成功；x64 窗口启动并正常关闭，exit code 0 | **Phase A 选择**；共享/可服务运行库，部署必须声明 .NET 与 Windows App Runtime 前置 |

决定已经回写 ADR-004 与 `docs/BUILD_WINDOWS.md`。它只固定开发基线；没有完成最终安装器、干净机、升级或卸载决策。

## 4. Phase A 实现

新增 `apps/settings-winui/` 下的官方 C# WinUI 3 blank app，并保持工程边界：

- `InputFlow.Settings.slnx` 与 C# 工程独立于 Cargo workspace。
- `TargetFramework` 固定为 `net10.0-windows10.0.26100.0`，最低 Windows 10.0.17763.0。
- `WindowsPackageType=None`、`WindowsAppSDKSelfContained=false`。
- Windows App SDK 2.5.1 与 Windows SDK BuildTools 10.0.28000.2705 使用明确版本。
- 主窗口只显示 `Native settings shell` 和 Phase A 说明；没有 Hook、配置保存、规则校验、agent/IPC 客户端或虚假在线状态。
- 关闭最后一个窗口遵循 WinUI 默认生命周期，没有托盘/隐藏窗口保持进程。
- 模板 manifest 中与本产品无关的 `systemAIModels` restricted capability 已移除。
- 根 `global.json` 固定 .NET SDK 10.0.401。

Phase A 当时没有创建 `inputflow-agent`、`inputflow-protocol`、Named Pipe、托盘、完整键盘、Schema v2、正式设置页面或鼠标方向规则。此后已继续完成 Phase B、Phase C、Phase D，以及 Phase E 的正式 UI 实现；鼠标方向边界仍未越过。

## 5. Phase B：完整键盘身份与 Schema v2

### 5.1 方案与决定

ADR-005 比较三种方案：只扩展 logical VK、只保存 scan code + extended、事件双身份且规则显式选择 match mode。最终选择第三种：

- 录制默认 logical，以保持快捷键语义和 v1 兼容；高级模式可切 physical。
- 每个事件保留 logical VK 与 physical scan/extended；同一事件同时命中时 exact physical 规则优先。
- held/repeat/release tombstone 的 tracking identity 优先 physical，使按住期间布局变化不会遗留状态或放出孤立 release。
- 布局显示名不作为配置 identity；未来非 Hook UI/agent 路径按当前 HKL 用 `MapVirtualKeyExW`/`GetKeyNameTextW`，OEM 字符预览可用谨慎隔离状态的 `ToUnicodeEx`。
- logical action 使用 VK；physical action 和捕获回放使用 `KEYEVENTF_SCANCODE`，保留 extended/down/up。文本输出是以后独立 action，不等同于按键输出。
- Unknown VK 保留在观察/回放路径但不能作为 logical 配置；有非零 scan 时可显式 physical 配置，不能静默映射为另一键。

### 5.2 代码实现

- engine 的 `Key` 补齐 Caps/Num/Scroll Lock、方向与导航、Windows OEM、numpad 0–9/运算/keypad Enter、PrintScreen/Pause/Apps、音量/媒体/浏览器，并加入 `Physical { scan_code, extended }`。
- `InputEvent` 提供 logical/physical identities、physical-first 候选顺序和 tracking key。matcher 把 rule identity 与 physical tracking 分离，并为 active logical prefix 增加同 physical release 识别。
- Windows keymap 对左右 Ctrl/Alt、左右 Shift、keypad Enter 和 extended 键显式归一化；debug 日志在显式 `--debug` 时显示 logical、scan 和 extended。
- 捕获回放优先原 scan；action 按 match mode 选择 VK/SCANCODE。过程中发现并修复既有缺陷：`event_to_input` 原先把 `down` 传给语义为 `up` 的参数，造成键盘回放方向反转。
- config 升级为严格 Schema v2；v1 字符串键只读兼容并在内存中迁移为 logical。`LoadedConfig` 返回 source schema/current document；新保存仅允许 v2，原子替换的 committed backup 保留 v1 回滚。
- 根 `fixtures/config/` 增加 v1/v2 golden contract，供未来 Rust/C# DTO 共同使用；另增加窄范围 `v2-phase-b-manual-acceptance.json`，固定 physical Oem1/logical CapsLock + F9 的失败回放与命中动作验收。

### 5.3 Caps Lock 和失败模型

自动测试覆盖：无规则 down/up 直通、候选后失败 FIFO 回放、命中消费与两个 release tombstone、repeat 保留、pause/F12/quit 共用 Hook-owner 冲刷、overflow 旁路、physical 优先、布局变化 release，以及 Caps 捕获 down/up 恰好生成一对正确方向的 scan-code INPUT。

这些测试还证明读取 immutable recording identity snapshot 后既有 Caps 规则仍照常命中。真实物理验收已经补充证明目标字符与硬件指示灯 toggle 次数正确；Phase D 已把 capture session 接入 IPC 并验证断线取消，Phase E 也已把它接入正式页面并覆盖草稿隔离、取消竞态和旧 session 过滤。随后正式页面实测录制 Caps、OEM、方向键、主 Enter 与 Fn 音量键并完成保存/重开读回；本机缺少 keypad Enter 和独立播放键，按硬件限制保留。

## 6. Phase C：产品级 Rust Agent Runtime

### 6.1 共用 runtime 与生命周期

新增 `crates/inputflow-runtime`，由它统一拥有配置、matcher、Hook/message-loop thread、bounded diagnostic logger、crash marker、状态与 shutdown。公开能力覆盖 `start/ready/status/pause/resume/apply_config/begin_capture/cancel_capture/shutdown`；`probe-cli` 已改为调用该 runtime，`apps/inputflow-agent` 只负责产品进程策略、日志、托盘和设置程序启动，不存在两份分叉 Hook 生命周期。

callback bridge 继续使用进程期 `OnceLock`，但不再把 matcher、logger sender 或 emergency key 固死为首次安装值：matcher/log sender 位于可替换的互斥 cell 中，emergency key 使用原子索引。callback 只在 Hook thread 生命周期内读取拥有所有权的进程期值，不持有调用方借用；runtime 重启前必须看到旧 Hook thread id 清零并重置 transient state，因此没有悬垂引用或两个 owner 并存。

所有会改变 matcher 时序状态的控制继续在 Hook owner 上串行：pause、resume、rule replacement、capture begin/cancel 都走控制消息和有界等待。正常退出仍先进入 shutdown/bypass，再按既有上限最多等待两秒，让已经消费的 release tombstone 排空，然后卸载 Hook。

### 6.2 热替换、持久化事务与 capture

规则替换顺序固定为：

1. runtime 权威校验 draft 并编译新 `RuleIndex`；失败时不写盘、不触碰 live matcher。
2. 原子保存正式配置，取得结构化 `SaveReport`（正式路径、backup、成功提交后的 cleanup warning）。
3. Hook owner 冲刷 pending、进入 bypass；若回放不完整则停止替换。冲刷成功后只替换 rule index/emergency key，保留旧规则已经产生的 consumed release tombstone；替换前不是 suspended 时才恢复 interception。
4. 第 3 步的控制结果区分 cancelled、failed 和 outcome-unknown。未开始已取消或确定失败时才立即尝试把旧配置原子写回；已开始但超时时保留 draft，用 request id 标记 pending 并在后台核对原请求终态。延迟成功时对齐 `current_config`/元数据，延迟失败时才回滚，终态丢失时保留 draft 并要求重启恢复。`ApplyReport` 保留 outcome、request id、save/runtime/rollback、错误与 `recovery_required`，不会把部分提交压成真假布尔值。

engine 确定性测试证明：旧规则命中后留下的 release tombstone 在替换后继续消费旧 release，而新 prefix 立即按新规则工作。config/runtime 测试分别覆盖 cleanup warning、验证失败零提交、保存先于替换、确定失败回滚、未开始取消，以及“已开始但超时、随后延迟成功”故障注入。最后一项明确断言超时时不回滚磁盘，延迟成功后 draft/current/runtime 收敛到同一新配置。

capture session 一次只允许一个，必须有 timeout，可取消；只观察首个非 injected、非 repeat 的键/鼠标 down，返回 logical + optional physical identity 或鼠标按钮。紧急键不成为 capture 结果，本程序注入也不会被录制。该观察发生在 matcher 决策前但不暂停、不预消费、不改变 matcher；timeout 由 Hook timer 驱动，shutdown 会返回明确终态。Phase D 已在 IPC 上承载该 API，没有复制录制状态机。

### 6.3 产品 agent 与 Win32 托盘

- `inputflow-agent` 使用 `Shell_NotifyIconW` 和独立 Win32 message thread；右键菜单含只读 Active/Paused 状态、Open Settings、Pause/Resume 和 Exit，双击图标打开设置。后续审阅删除了只由托盘操作更新的 `TRAY_PAUSED`；tooltip/菜单现直接读取 runtime 权威 suspended，F12、托盘、输出失败和 overflow 的所有状态转换都通过同一非阻塞通知刷新 tooltip。
- pause/resume 直接复用 runtime 的 Hook-owner 控制路径。注册并处理 `TaskbarCreated`，Explorer 重建通知区后会重新 `NIM_ADD`；后续真实重启 Explorer 已确认图标、Active 状态和菜单自动恢复，agent 进程未重启。
- 当前 session 使用 `Local\\InputFlow.Agent.v1` named mutex 单实例。第二实例不安装第二套 Hook，而是请求启动 `InputFlow.Settings.exe`；找不到时在本地日志写入明确错误并以 3 退出。
- Release 使用 `windows_subsystem = "windows"`；实测 PE32+ subsystem 为 2（Windows GUI），不创建控制台窗口。agent 不依赖 .NET/WinUI/WebView。
- 本地日志默认只记录生命周期、聚合计数与匿名 matched 提示；逐输入 logical/scan/extended 以及 rule identity 必须显式 `--debug-input`。日志启动时超过 1 MiB 会截断，Hook 到 logger 仍是 bounded `try_send`。

### 6.4 自动与 Windows smoke 证据

Phase C 最终 Rust 命令：

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p inputflow-agent
cargo build -p inputflow-agent --release
```

自动结果在后续审阅修复后为 128/128：engine 73、config 25、windows 22、runtime 5、agent 3；Clippy 在 `-D warnings` 下通过。Debug agent 用独立 target 配置完成 100 次 pause/resume、apply current config、begin/cancel capture 和 clean shutdown；审阅修复后的 Release agent 又重复同一 100 轮 smoke。最新结果为 `running` marker 已删除、agent 进程已退出、0 output failed、0 dropped、Hook/logger 无 panic。该 smoke 没有输入样本，不能冒充物理 capture 或键鼠回归。

Release 托盘/Hook 限时启动并正常退出。资源短样本如下：

| 场景 | CPU 观察 | Working set | Private bytes | Threads | Handles |
|---|---:|---:|---:|---:|---:|
| 空闲约 2 秒 | 累计 0.109375 s | 10,340 KiB | 1,652 KiB | 6 | 143 |
| 空闲约 6 秒 | 累计 0.15625 s | 10,340 KiB | 1,652 KiB | 6 | 143 |
| 1000 次 pause/resume 后 | 累计 0.500000 s | 10,000 KiB | 1,760 KiB | 7 | 145 |
| 再请求打开设置 3 次并等待 | 累计 0.531250 s | 10,000 KiB | 1,760 KiB | 7 | 145 |
| 设置窗口正常关闭后的 settled 样本 | 累计 0.546875 s | 10,000 KiB | 1,760 KiB | 7 | 145 |

3 个第二实例均在 5 秒内以 0 退出，并各自启动一个 Phase A 设置进程；向三个已验证路径/窗口发送正常 `WM_CLOSE` 后均退出。打开请求前后 agent working set、private bytes、threads、handles 的增长都为 0；主 agent 最终 exit code 0。以上只是本机 Release 短样本，不能取代长稳态/高负载门槛。

### 6.5 托盘人工验收

2026-09-30 用户在 Release agent（PID 9688）上逐项确认结果与预期一致：Active tooltip、右键 `Status: Active`、Pause 后 `Status: Paused`、Resume 后恢复 Active、`Open Settings` 启动 WinUI 窗口、关闭设置后 agent 继续常驻，以及托盘 Exit 后图标/进程消失。

本地证据与用户观察一致：日志记录 `tray: paused held=0 output_complete=true`、`tray: resumed`、`settings launch requested`，最终 `stopped: observed=22 output_sent=0 output_failed=0 output_dropped=0 hook_panicked=false logger_panicked=false`；PID 9688 已不存在，`%LOCALAPPDATA%\InputFlow\running` marker 已清除。该验收证明托盘菜单路径和干净退出；后续 `TaskbarCreated` 的真实 Explorer 重启恢复也已通过。

### 6.6 Phase C 修改与产物

- 新增：`crates/inputflow-runtime/{Cargo.toml,src/lib.rs}`、`apps/inputflow-agent/{Cargo.toml,src/main.rs}`、`crates/inputflow-windows/src/platform/shell.rs`。
- runtime/platform：修改 workspace manifest/lock、`inputflow-windows` platform module/windows bridge、engine matcher、config API。
- 共用入口：修改 `probe-cli` manifest/main，使其调用共享 runtime。
- 记录：更新 README、PROJECT_PLAN、BUILD_WINDOWS、ADR-000、ADR-004、research-log 与本执行记录。
- 实际 Rust agent 产物：`target/debug/inputflow-agent.exe` 与 `target/release/inputflow-agent.exe`；审阅修复后 Release 文件大小 1,182,720 bytes，PE32+ Windows GUI subsystem。

### 6.7 Phase C 后续审阅：两个一致性问题

2026-09-30 再次审阅确认用户报告的两个问题都真实存在：

1. F12 由 Hook callback 直接切换 `BYPASS`，原托盘却只在菜单操作和 smoke 中更新独立 `TRAY_PAUSED`。因此 F12 后实际状态与 tooltip/菜单必然分叉，且点击显示为 Pause 的菜单会根据 runtime 真值执行 resume。修复删除了 `TRAY_PAUSED`，展示与命令都读取同一 `BYPASS`；`set_bypassed` 是唯一状态转换入口，修改时递增 revision 并向托盘线程发送非阻塞 refresh。
2. 原 `request_control` 的错误文本已区分“未开始已取消”和“已开始、可能延迟完成”，但 `apply_transaction` 把所有 `Err` 都立即回滚磁盘，因此用户给出的时序能产生“磁盘/内存为旧、live matcher 为新”。修复保留已开始请求的 receiver 和 request id，用后台 reconciliation 根据真正终态提交内存新配置或回滚磁盘旧配置，期间阻止第二次 apply。

故障注入使 running 请求在 1 ms 内超时，再人工送入延迟 `Applied`。测试断言超时后只落盘 draft、没有旧配置 rollback，核对后 `current_config`、rule count 与 reconciliation 状态均收敛到新配置。另一测试将请求保持在 pending，确认超时会 CAS 取消且 Hook 不会执行它。

F12 托盘修复随后完成真实人工复验：Release agent PID 520 初始显示 Active/Pause；按 F12 后 tooltip 与右键菜单正确变为 Paused/Resume；点击 Resume 后恢复 Active/Pause，最后从托盘正常 Exit。用户确认可见结果全部符合预期；日志对应记录 `suspended ON`、`tray: resumed` 和 clean stop，最终 observed=20、output failed/dropped=0、Hook/logger 无 panic。PID 520 已不存在，`running` marker 已清除。

## 7. Phase D：版本化 Named Pipe

### 7.1 决策与安全边界

新增并接受 `docs/decisions/ADR-006-版本化Named-Pipe协议与安全边界.md`。比较 JSON lines、4-byte little-endian length-prefix UTF-8 JSON 和 message-mode pipe 后选择 length-prefix；协议 v1 的 frame 上限为 1 MiB。每个 request 必须包含版本和最多 128 bytes 的字符串 request ID，首个 request 必须是 handshake；每连接最多登记 4096 个 ID，重复 ID 明确拒绝且不重放 mutation。

pipe 名含当前 Windows session id。`inputflow-windows::platform::pipe` 从当前进程 token 取得用户 SID，构造只允许该用户与 SYSTEM 的 protected DACL，并使用 `PIPE_REJECT_REMOTE_CLIENTS`、overlapped connect/read/write 和共享 shutdown event。该模块拥有所有新增 Win32 `unsafe`；协议解析、runtime 调用和配置事务保持安全 Rust。Hook callback 不等待 pipe、磁盘或 UI。

### 7.2 Rust server 与 agent 接线

新增 `inputflow-protocol`，包含严格 serde DTO、frame codec、并发连接 server、有界 event hub、错误码与兼容测试。server 只有在首个安全 pipe instance 已创建后才报告 ready；最多 8 个连接，每订阅者队列容量 32，发布使用 `try_send`，慢客户端不会反压 runtime。event ID 单调递增，客户端可用间隙识别丢弃；heartbeat 不伪造新状态。

agent handler 实现 handshake、get_status、get_config、validate_config、apply_config、pause、resume、get_stats、begin_capture、cancel_capture 和 subscribe_events。它只调用 Phase C 的 runtime/config public API：apply 仍保持验证/编译 → 原子保存 → Hook-owner replace，并返回 save/runtime/rollback 结构；pause/resume 和 F12/tray 共用同一个权威状态。连接断开不修改规则或 pause 状态，只取消由该连接拥有的 capture；shutdown 广播终态并中断 overlapped I/O。

### 7.3 C# client 与共享 contract

新增 `InputFlow.Protocol` .NET class library：同一 framing/上限/session pipe 名，连接后自动 handshake，默认 3 秒 deadline，mutation 不自动重试，并覆盖 Phase D 全部方法和事件流。Phase E 已在其上增加应用级 control/event 协调器与正式页面绑定，没有绕开或复制该协议边界。

`fixtures/protocol/v1` 保存 handshake request/response、get-config response、error response 和 capture event。Rust 反序列化同一 fixture；无第三方测试包的 `InputFlow.Protocol.ContractTests` 验证 C# framing 和 fixture 语义。真实 agent live contract 进一步覆盖 status/config/validate/apply current config、pause 事件、resume、stats、begin/cancel capture，以及第二客户端开始 capture 后直接断线、主客户端轮询确认 capture 自动结束。

### 7.4 Phase D 修改与验证

- Rust：`crates/inputflow-protocol`、`inputflow-windows/src/platform/pipe.rs`、agent `ipc.rs`、runtime observed event stats 与 workspace feature/dependency 接线。
- C#：`InputFlow.Protocol`、`InputFlow.Protocol.ContractTests`、solution/project reference。
- Contract：`fixtures/protocol/v1` 与 ADR-006。
- 自动结果：143/143（agent 4、config 25、engine 73、protocol 12、runtime 5、windows 24）；fmt 与 Clippy `-D warnings` 通过；probe、agent Debug/Release 和 C# Debug/Release 构建通过。
- Windows live：C# fixture runner 通过 6 项；连接真实 Debug agent 的跨语言 live contract 全部通过，client/agent exit code 均为 0。

## 8. 验证证据

### 8.1 Rust 回归

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p probe-cli
cargo build -p inputflow-agent
cargo build -p inputflow-agent --release
```

Phase B 当时结果为 113/113。Phase C 后续审阅修复后结果为 128/128。Phase D 最终结果为 143/143（agent 4、config 25、engine 73、protocol 12、runtime 5、windows 24、probe 0），Clippy 在 `-D warnings` 下无警告，probe-cli 与 inputflow-agent Debug/Release 构建成功。

Phase A 曾执行 `pause → resume → stats → quit` lifecycle smoke。Phase B 又用 `fixtures/config/v2-valid.json` 执行 clean-quit smoke：加载 1 条 physical/logical rule，Hook/timer 安装与清理成功，exit code 0。运行先报告 2026-09-27 遗留的 abnormal marker；给予程序正常 LocalAppData 权限后再次 clean quit，确认 `running` marker 已删除。两次 smoke 都没有物理输入，callback 样本为 0，不是桌面输入或性能验收。

### 8.2 WinUI 构建

```powershell
dotnet restore .\apps\settings-winui\InputFlow.Settings.slnx
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Debug --no-restore
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Release --no-restore
dotnet run --project .\apps\settings-winui\InputFlow.Protocol.ContractTests\InputFlow.Protocol.ContractTests.csproj -c Debug --no-build
```

结果：删除既有 `bin`/`obj` 后重新 restore/build 成功；Debug 与 Release 都是 0 warning、0 error。干净的 framework-dependent 输出不包含 `coreclr.dll`/`hostfxr.dll`，排除了先前 self-contained 实验产物混入。

最终 x64 产物：

- Debug：`apps/settings-winui/InputFlow.Settings/bin/Debug/net10.0-windows10.0.26100.0/win-x64/InputFlow.Settings.exe`
- Release：`apps/settings-winui/InputFlow.Settings/bin/Release/net10.0-windows10.0.26100.0/win-x64/InputFlow.Settings.exe`

### 8.3 WinUI 生命周期实测

最终 framework-dependent Debug 产物的观测摘要：

```text
WINDOW_READY pid=1072 handle=197352 title=InputFlow.Settings
WM_CLOSE_POSTED=True
PROCESS_EXITED=true exit_code=0
```

这只证明当前 x64 本机能显示原生窗口并在正常关闭后退出。该 Phase A 测试当时未检查 agent 独立存活，因为 agent 尚未实现。

### 8.4 Phase B Windows 输入状态

`Get-WinUserLanguageList` 实际列出：

```text
en-US       0409:00000409
zh-Hans-CN  Microsoft Pinyin
```

本轮使用真实物理键、普通权限记事本目标窗口和 probe `--debug` 完成以下验收。逐键内容只保留 identity、状态与统计，不保存用户输入文本：

| 输入状态/场景 | 完整手势 | 预期 | 实际观察 | 结果 |
|---|---|---|---|---|
| en-US 直通观察 | `Oem1`；Caps OFF→按 Caps→`a`→按 Caps→`a`；主 Enter；VolumeUp | 各一对 down/up；`A` 后 `a`；目标输入正确 | `Oem1 scan=0x27 extended=false`、`CapsLock 0x3A false`、`A 0x1E false`、主 Enter `0x1C false`、VolumeUp `0x30 true`；记事本全部正确 | 通过 |
| Microsoft Pinyin 直通观察 | 同一 `Oem1`；Caps OFF→ON/OFF 各输入 `a` | 常用输入状态下 identity 稳定，字符与 Caps 状态正确 | identity 与 en-US 相同；用户确认第一个 `A`、第二个 `a`，记事本全部正确 | 通过 |
| Pinyin physical OEM 失败 | 单独按下/松开 `Oem1`，不按 F9 | 暂存后按 scan 原样回放，只出现一个标点 | seq 22/23 后 `replay 2 held event(s)`，`inserted=2 requested=2`；记事本只有一个分号，无快捷动作 | 通过 |
| Caps 失败回放 | 初始灯灭；单按 Caps→`a`；再单按 Caps→`a` | 两轮各切换一次，依次大写/小写，最终灯灭 | 两轮均各收到 `0x3A` down/up 并回放 2/2；用户确认字符和指示灯正确 | 通过 |
| logical Caps 命中消费 | 灯灭；按住 Caps，按下/松开 F9，最后松 Caps | Caps 不送入目标，只执行一次 `C` action，灯保持灭 | 含多个 physical repeat，但规则只匹配一次；action `inserted=2 requested=2`；仅出现小写 `c`，灯灭 | 通过 |
| physical OEM 命中消费 | 英文；按住 `;`，按下/松开 F9，最后松 `;` | physical `scan=0x27` 命中，只输出 `c` | 含 repeat 的 Oem1 序列只匹配一次；action 2/2；仅出现小写 `c`，无分号/快捷动作 | 通过 |
| F12 pending 恢复 | 灯灭；Caps down pending 时按 F12，松 Caps，输入 `a`；再按 F12 恢复，单按 Caps，输入 `a` | 暂停先冲刷 held down，释放直通；恢复后 matcher 正常；最终灯灭 | pending Caps 回放 1/1 后 suspended ON，得到大写 `A`；第二次 F12 resumed，Caps 失败回放 2/2，得到小写 `a`；无卡键/附带动作 | 通过 |

带规则探针正常退出时报告 `observed 1209 events; 7 output batch(es) sent, 0 failed, 0 dropped`。callback debug 统计为 p50 3us、p95 88us、p99 214us、max 17079us；该运行开启逐事件 debug 且包含人为长按和大量交互输入，不能作为 Release 性能门槛。

本机物理覆盖限制：实际 Enter 为主键区 Enter，不是 keypad Enter；用户键盘没有确认独立 Apps/Menu，PrintScreen、Pause、Num/Scroll Lock、numpad、其余媒体键未逐一实测。它们已有 mapping/round-trip/replay 自动测试，但不能写成此硬件已实测。

### 8.5 Phase B 人工验收复现

```powershell
.\target\debug\probe-cli.exe --debug --config .\fixtures\config\v2-phase-b-manual-acceptance.json
```

配置只含两条规则：physical `scan_code=39, extended=false` + F9，以及 logical CapsLock + F9；二者命中均输出 `C`，F12 为紧急旁路。单独松开首键验证失败回放，按住首键再按 F9 验证命中消费。每个 Caps 场景必须先记录初始灯状态，并同时检查目标字符与最终灯状态。overflow、quit pending 和精确 INPUT flags 已由确定性测试覆盖；不要用长时间真实键盘洪泛替代自动 overflow 测试。Phase E 页面已经接线，但页面上的真实物理录制/读回矩阵仍待执行。

## 9. Phase E：正式 WinUI 3 设置程序

### 9.1 设计与 E0 Schema v3

页面修改前新增 `docs/PHASE_E_UI_DESIGN.md`，固定信息架构、正式快照/草稿边界、control/event 双连接、保存与 reconciliation 事件序列、capture 竞态处理和验收分层。ADR-007 解决单规则启停的持久化缺口：

- 配置升级为严格 Schema v3，每条规则保存必需的 `enabled`；v1/v2 继续可读并确定迁移为 `enabled: true`，新保存只写 v3。
- 所有规则仍校验 ID、trigger/action、键身份和 timeout；重复 ID 跨启用/禁用规则拒绝。只有启用规则参加紧急键可达性、冲突和运行时索引，`rule_count` 只统计启用规则。
- wire protocol 保持 v1，handshake 的 schema 提升到 3、capability 更新为 `config_v3`；旧 agent 会被设置程序明确拒绝。
- 新增 v3 golden fixture 与 v1/v2→v3 Rust/C# contract；启停继续走完整 apply/Hook-owner replace，测试覆盖 pending 冲刷及旧 consumed-release tombstone 保留。

### 9.2 E1–E3：连接、规则草稿、录制与保存

- `InputFlow.Settings.Core` 集中承载强类型配置、正式快照/深拷贝草稿、事件序列、capture generation、连接状态和保存状态机；WinUI code-behind 不复制 Rust 权威校验。
- 应用级 coordinator 长期持有独立 control/event client。事件流在订阅前先建立并 handshake；断线重连、event ID 缺口会读取 status/config 重同步。连接展示区显示真实 phase、暂停状态、启用规则数、reconciliation 与最后错误。
- 快捷规则页支持分组筛选、创建、编辑、删除、启停及未保存提示。编辑器覆盖当前后端四类 trigger（key chord、key+mouse、hold、hold+mouse）和可变长 key-chord action；取消编辑不会触碰正式快照。
- `KeyPicker` 保存稳定 logical/physical identity，用当前 HKL 的 `MapVirtualKeyExW`/`GetKeyNameTextW` 仅生成显示名。录制每次只绑定一个字段/session；Esc 与按钮取消使本地 generation 立即失效，迟到/旧 session/type mismatch 不写入字段，physical 仅在非零 scan 时可用。
- 保存先比较当前 agent 配置并提示外部变化，再调用 validate、单次 apply 和 get_config 读回。验证失败零 mutation；超时或断线绝不自动重试，保留 draft 并根据读回显示已应用、回滚或需要恢复。

### 9.3 E4：设置、诊断、关于与窗口生命周期

- 设置页接入紧急键草稿、agent 实际暂停/恢复和手动刷新统计；无统计样本显示“暂无样本”。登录启动、系统辅助功能、PowerToys、配置恢复和未来鼠标方向没有用假开关占位。
- 关于页从 assembly 获取版本并说明 agent/UI 架构与支持边界。顶层使用紧凑 `NavigationView`：快捷规则、设置、关于；连接状态和操作结果分别展示，避免后续状态事件覆盖保存回执。
- settings 以当前会话 mutex 保证单实例；第二进程退出并恢复/前置现有窗口。脏草稿关闭时提供保存/丢弃/继续编辑。连接清理有上限；agent 先退出时窗口仍能关闭，不遗留 settings 进程。
- 新增 `scripts/build-windows.ps1` 作为 Cargo 与 .NET 的联合构建/测试入口。

### 9.4 自动化与真实 Windows 观察

| 证据层 | 结果 |
|---|---|
| Rust | 151/151（agent 4、config 28、engine 74、protocol 12、runtime 6、windows 27）；fmt、Clippy `-D warnings`、probe/agent Release build 通过 |
| C# contract/core | protocol fixture 6 项；settings core 11 项，覆盖三代 Schema、草稿隔离、保存成功/验证失败/外部变化、timeout reconciliation、capture 旧 session、begin 返回前字段互斥/取消意图、规则启用绑定和 event gap |
| .NET 构建 | solution Debug/Release 均为 0 warning、0 error |
| 真实 agent contract | Release agent 与 C# live contract 通过；隔离配置下 agent 正常退出 |
| UI 生命周期/UIA | 离线窗口与 settings 单实例通过；在线能读到 connected/ready；规则启停保存后 v3 文件持久化 `enabled: false` 且启用数归零；空配置创建 key-chord 并保存成功；UI 关闭后 agent 仍存活；agent 先退出后 UI 仍能正常关闭 |

UIA 与实机过程中发现并修复多项真实缺陷：连接状态事件覆盖“保存完成”、agent 先停止时窗口清理延迟、`begin_capture` 返回前目标覆盖/无法取消、列表重建误清全部规则启用状态、同步 `SendInput` 持有 matcher 锁导致物理重入死锁、Medium→High 目标被 API 返回值误报 Complete，以及设置页固定 760 px 宽度。对应修复均保留权威状态/有界生命周期；Windows 层新增 UIPI token integrity/UIAccess 前置判定，设置页和规则行 UIA/响应式布局也已复验。

### 9.5 Phase E Windows 联合验收

- 正式页面已用真实硬件录制并读回 Caps Lock、OEM、四个方向、主 Enter 及 Fn Lock 后的 VolumeMute/VolumeDown/VolumeUp；Esc 取消不改写字段。keypad Enter 与独立播放键因本机硬件不存在而未伪造实测。
- `hold`、`key_mouse_button`、`hold_mouse_button` 已通过 UI 创建、保存、重开读回和物理命中；规则启用状态、F12/UI/托盘双向同步及鼠标释放墓碑均通过。
- 纯键盘焦点流程、UIA 名称、高对比度、125%/150% 和恢复 200% 缩放通过。中文 Narrator 实际语音受本机中文辅助语音环境限制；UIA 枚举确认所有可聚焦控件名称非空且非通用。
- 冷启动/开窗/关窗短资源边界与 Release 五分钟长稳态均完成；最终 stats p99 16.032 ms、callback p99 328 µs，输出 failed/dropped 增量均为 0，线程和句柄无持续增长。
- M6-01～M6-07 的菜单/墓碑、四类 repeat/队列满、物理回放顺序、UIPI 完整/零写入、高负载、修饰/布局/光标以及两秒退出边界均完成；Explorer 真实重启后的托盘恢复通过。partial SendInput 因无可控来源保留为自动故障注入证据，不写成实机通过。

完整逐步证据与原始日志索引见 `M7-Phase-E与M6-Windows实机验收记录.md` 和 `target/acceptance/20261001-013826-physical/`。

## 10. 未执行与已知限制

- Packaged 启动未验证；Developer Mode 未启用。
- Phase A 工程只声明 x64；x86 与 ARM64 尚未纳入支持范围。
- 干净机首次安装、缺运行库行为、升级、卸载、签名和最终分发未验证。
- 本机没有数字小键盘/keypad Enter 和独立播放媒体键；这些物理输入未实测。PrintScreen、Pause、Apps/Menu、Num/Scroll Lock 及全部媒体键也未逐一做本机物理状态矩阵，自动覆盖不等于硬件覆盖。
- 中文 Narrator 语音朗读因本机中文辅助语音环境不足未完成听觉判定；应用侧 UIA 名称、键盘焦点、高对比度和缩放已单独通过。
- partial SendInput 没有可控稳定触发源，实机只覆盖普通完整插入及提升目标 replay/action 零写入；partial 分支由确定性故障注入覆盖。
- Phase F/M8 鼠标方向尚未开始。
- Phase B 未逐一实测 PrintScreen、Pause、Apps/Menu、keypad Enter、Num/Scroll Lock 和全部媒体键；自动覆盖不等于本机硬件覆盖。

## 11. 阶段状态

| 阶段 | 状态 | 下一门槛 |
|---|---|---|
| Phase A：构建链与工程边界 | **完成** | 本记录、ADR、README/BUILD_WINDOWS 与可重复 smoke 均已落地 |
| Phase B：完整键盘身份与 Schema v2 | **完成** | ADR、113 项自动测试、en-US/Microsoft Pinyin OEM、Caps 目标字符/指示灯与 F12 pending 恢复均有证据；正式 UI capture 页面属 Phase E |
| Phase C：产品级 Rust agent runtime | **完成** | 共用 runtime、Hook-owner 热替换、结构化 apply/save、capture、托盘、单实例、资源 smoke 与托盘人工验收已落地；F12 同步、超时核对及 Explorer 真重启恢复均已实机复验 |
| Phase D：版本化 Named Pipe | **完成** | ADR-006、安全 pipe、Rust server/agent handler、C# client、共享 fixtures、143 项 Rust 测试与真实跨语言 live contract 已落地 |
| Phase E：正式 WinUI 设置程序 | **完成（含明确硬件/环境限制）** | Schema v3、正式页面、151 项 Rust、17 项 C#、物理 capture/规则流程、UIA/高对比度/缩放、五分钟资源及 M6 实机矩阵均有证据 |
| Phase F / M8：鼠标方向 | 未执行 | 独立 ADR、算法测试和高频输入证据 |

## 12. 下一次最小任务

下一次最小功能任务可进入 Phase F/M8：先建立鼠标方向规则 ADR，固定激活条件、阈值/偏轴/超时和 O(1) 热路径，再写纯算法测试，最后才接真实高频移动验收。若优先交付产品，则先单独规划 packaged/安装器、干净机运行库、升级/卸载与签名，不把它们混入已经完成的 Phase E 功能验收。
