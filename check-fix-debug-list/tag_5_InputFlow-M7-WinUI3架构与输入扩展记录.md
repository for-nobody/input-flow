# InputFlow M7/M8：WinUI 3 架构与输入扩展执行记录

> 日期：2026-09-30
> 对应任务：`tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md`
> 本次累计范围：Phase A、Phase B 与 Phase C 已完成；Phase B 包含真实物理输入验收，Phase C 包含共享 runtime、产品 agent/托盘、自动测试与资源 smoke；Phase D–E 与 Phase F/M8 未执行

## 1. 基线核对

| 项目 | 结果 |
|---|---|
| Git 分支 | `main` |
| 开工 HEAD | `2ead0a9677d2eb6baa2377d4c876ee6a7d383b90`（`2ead0a9 update again with new develop path of this project`） |
| Phase C 开工 HEAD | `e8a4871e5adb829d3f64ce2d8ee366e652c6e5db`（`feat: complete M7 native shell and keyboard identity v2`），与 `origin/main` 同步，工作区干净 |
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

Phase A 当时没有创建 `inputflow-agent`、`inputflow-protocol`、Named Pipe、托盘、完整键盘、Schema v2、正式设置页面或鼠标方向规则。此后已继续完成 Phase B 和 Phase C；IPC、正式 UI 与鼠标方向边界仍未越过。

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

这些测试还证明读取 immutable recording identity snapshot 后既有 Caps 规则仍照常命中。真实物理验收已经补充证明目标字符与硬件指示灯 toggle 次数正确；设置录制 session 仍属于 Phase D/E，目前只能通过 probe `--debug` 观察 normalized candidate，尚不能把“设置录制不改变运行规则”写成正式 UI 端到端通过。

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
4. 第 3 步失败时尝试把旧配置重新原子写回，并返回带 save/runtime/rollback、错误与 `recovery_required=true` 的 `ApplyReport`。不会把部分提交压成真假布尔值。

engine 确定性测试证明：旧规则命中后留下的 release tombstone 在替换后继续消费旧 release，而新 prefix 立即按新规则工作。config/runtime 测试分别覆盖 cleanup warning、验证失败零提交、保存先于替换，以及运行时失败后旧配置 rollback。

capture session 一次只允许一个，必须有 timeout，可取消；只观察首个非 injected、非 repeat 的键/鼠标 down，返回 logical + optional physical identity 或鼠标按钮。紧急键不成为 capture 结果，本程序注入也不会被录制。该观察发生在 matcher 决策前但不暂停、不预消费、不改变 matcher；timeout 由 Hook timer 驱动，shutdown 会返回明确终态。Phase D 只应在 IPC 上承载该 API。

### 6.3 产品 agent 与 Win32 托盘

- `inputflow-agent` 使用 `Shell_NotifyIconW` 和独立 Win32 message thread；右键菜单含只读 Active/Paused 状态、Open Settings、Pause/Resume 和 Exit，双击图标打开设置，tooltip 同步状态。
- pause/resume 直接复用 runtime 的 Hook-owner 控制路径。注册并处理 `TaskbarCreated`，Explorer 重建通知区后会重新 `NIM_ADD`；真实重启 Explorer 的人工观察尚未执行。
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

自动结果为 124/124：engine 73、config 25、windows 20、runtime 3、agent 3；Clippy 在 `-D warnings` 下通过。Debug agent 用独立 target 配置完成 100 次 pause/resume、apply current config、begin/cancel capture 和 clean shutdown：exit code 0、`running` marker 已删除、0 output failed、0 dropped。该 smoke 没有输入样本，不能冒充物理 capture 或键鼠回归。

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

本地证据与用户观察一致：日志记录 `tray: paused held=0 output_complete=true`、`tray: resumed`、`settings launch requested`，最终 `stopped: observed=22 output_sent=0 output_failed=0 output_dropped=0 hook_panicked=false logger_panicked=false`；PID 9688 已不存在，`%LOCALAPPDATA%\InputFlow\running` marker 已清除。该验收证明托盘菜单路径和干净退出；真实重启 Explorer 后图标恢复仍未执行。

### 6.6 Phase C 修改与产物

- 新增：`crates/inputflow-runtime/{Cargo.toml,src/lib.rs}`、`apps/inputflow-agent/{Cargo.toml,src/main.rs}`、`crates/inputflow-windows/src/platform/shell.rs`。
- runtime/platform：修改 workspace manifest/lock、`inputflow-windows` platform module/windows bridge、engine matcher、config API。
- 共用入口：修改 `probe-cli` manifest/main，使其调用共享 runtime。
- 记录：更新 README、PROJECT_PLAN、BUILD_WINDOWS、ADR-000、ADR-004、research-log 与本执行记录。
- 实际 Rust agent 产物：`target/debug/inputflow-agent.exe` 与 `target/release/inputflow-agent.exe`；Release 文件大小 1,154,560 bytes，PE32+ Windows GUI subsystem。

## 7. 验证证据

### 7.1 Rust 回归

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p probe-cli
```

Phase B 当时结果为 113/113。Phase C 最终结果更新为 124/124（engine 73、config 25、windows 20、runtime 3、agent 3、probe 0），Clippy 在 `-D warnings` 下无警告，probe-cli 与 inputflow-agent Debug/Release 构建成功。

Phase A 曾执行 `pause → resume → stats → quit` lifecycle smoke。Phase B 又用 `fixtures/config/v2-valid.json` 执行 clean-quit smoke：加载 1 条 physical/logical rule，Hook/timer 安装与清理成功，exit code 0。运行先报告 2026-09-27 遗留的 abnormal marker；给予程序正常 LocalAppData 权限后再次 clean quit，确认 `running` marker 已删除。两次 smoke 都没有物理输入，callback 样本为 0，不是桌面输入或性能验收。

### 7.2 WinUI 构建

```powershell
dotnet restore .\apps\settings-winui\InputFlow.Settings.slnx
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Debug --no-restore
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Release --no-restore
```

结果：删除既有 `bin`/`obj` 后重新 restore/build 成功；Debug 与 Release 都是 0 warning、0 error。干净的 framework-dependent 输出不包含 `coreclr.dll`/`hostfxr.dll`，排除了先前 self-contained 实验产物混入。

最终 x64 产物：

- Debug：`apps/settings-winui/InputFlow.Settings/bin/Debug/net10.0-windows10.0.26100.0/win-x64/InputFlow.Settings.exe`
- Release：`apps/settings-winui/InputFlow.Settings/bin/Release/net10.0-windows10.0.26100.0/win-x64/InputFlow.Settings.exe`

### 7.3 WinUI 生命周期实测

最终 framework-dependent Debug 产物的观测摘要：

```text
WINDOW_READY pid=1072 handle=197352 title=InputFlow.Settings
WM_CLOSE_POSTED=True
PROCESS_EXITED=true exit_code=0
```

这只证明当前 x64 本机能显示原生窗口并在正常关闭后退出。未检查 agent 独立存活，因为 agent 尚未实现。

### 7.4 Phase B Windows 输入状态

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

### 7.5 Phase B 人工验收复现

```powershell
.\target\debug\probe-cli.exe --debug --config .\fixtures\config\v2-phase-b-manual-acceptance.json
```

配置只含两条规则：physical `scan_code=39, extended=false` + F9，以及 logical CapsLock + F9；二者命中均输出 `C`，F12 为紧急旁路。单独松开首键验证失败回放，按住首键再按 F9 验证命中消费。每个 Caps 场景必须先记录初始灯状态，并同时检查目标字符与最终灯状态。overflow、quit pending 和精确 INPUT flags 已由确定性测试覆盖；不要用长时间真实键盘洪泛替代自动 overflow 测试。正式设置录制端到端测试等待 Phase D/E capture session。

## 8. 未执行与已知限制

- M6 人工矩阵仍未完成：真实右键菜单/释放墓碑、自动重复、物理回放顺序、UIPI、100k/高负载、修饰键/布局、鼠标位置和两秒 shutdown 边界均不得写成通过。
- Packaged 启动未验证；Developer Mode 未启用。
- Phase A 工程只声明 x64；x86 与 ARM64 尚未纳入支持范围。
- 干净机首次安装、缺运行库行为、升级、卸载、签名和最终分发未验证。
- WinUI 自动化/UIA、无障碍、高对比度、缩放和资源基线尚未进入正式 UI 阶段。
- 当前设置 shell 不具备产品功能；Phase C agent 已完成，但 Phase D IPC、Phase E 正式设置和 Phase F/M8 均未完成。
- Phase C 托盘菜单的 Active/Pause/Resume/Open Settings/Exit 已由用户人工验证；真实 Explorer 重启后的图标恢复观察仍未执行，代码路径不能替代该项证据。
- Phase B 未逐一实测 PrintScreen、Pause、Apps/Menu、keypad Enter、Num/Scroll Lock 和全部媒体键；自动覆盖不等于本机硬件覆盖。

## 9. 阶段状态

| 阶段 | 状态 | 下一门槛 |
|---|---|---|
| Phase A：构建链与工程边界 | **完成** | 本记录、ADR、README/BUILD_WINDOWS 与可重复 smoke 均已落地 |
| Phase B：完整键盘身份与 Schema v2 | **完成** | ADR、113 项自动测试、en-US/Microsoft Pinyin OEM、Caps 目标字符/指示灯与 F12 pending 恢复均有证据；正式 UI capture 属 Phase D/E |
| Phase C：产品级 Rust agent runtime | **完成** | 共用 runtime、Hook-owner 热替换、结构化 apply/save、capture、托盘、单实例、124 项测试、资源 smoke 与托盘人工验收已落地；Explorer 实际重启仍如实列为未执行 |
| Phase D：版本化 Named Pipe | 未执行 | 先写 ADR-006，固定 framing/ACL/超时/事务 |
| Phase E：正式 WinUI 设置程序 | 未执行 | 等待 agent/protocol，不复制权威校验或安装 Hook |
| Phase F / M8：鼠标方向 | 未执行 | 独立 ADR、算法测试和高频输入证据 |

## 10. 下一次最小任务

下一次进入 Phase D，最小任务是先写 ADR-006，比较 JSON length-prefix、JSON lines 与其他 framing，并固定协议版本、request id、最大消息、当前用户 ACL、超时、取消、断线、重复请求和 shutdown 语义；随后只实现协议与 Named Pipe server/client contract，不同时展开正式 UI。M6 尚未完成的菜单/墓碑、UIPI 与高负载真实矩阵仍须如实保留。
