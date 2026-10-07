# InputFlow Windows 构建与运行基线

> 状态：M7 Phase A–E 已完成；Phase F 代码与自动验证已完成、Windows 物理验收待执行（更新于 2026-10-08）。当前已有共享 Rust runtime、常驻 Win32 agent/托盘、完整键盘身份/Schema v4、版本化 Named Pipe，以及正式 WinUI 3 设置程序。缺失硬件、中文 Narrator 语音环境、partial SendInput、Phase F 物理输入和部署矩阵继续明确单列。

## 0. 首版交付修订（2026-10-08）

本文保留 Phase E 的历史开发基线，并在第 3、4、9 节追加当前结果。Phase F 的 ADR-008、方向 matcher／Hook、Schema v4、IPC capability 和 WinUI 编辑／有限预览已经实现；当前顺序为 F4／F5 物理收口 → G-PRE → H → RC → 首个 Pre-release → G-POST。鼠标方向必须在首版，24／72 小时长测安排在首版发布后。

Phase A–E 的开发构建成功不代表 `dotnet publish` 最终目录、依赖、自启动、升级／移除或干净环境已经验收。下方的 framework-dependent 是当前实现，首版 self-contained 只是待验证的 H 方案。

## 1. 目标产物与当前进度

| 产物 | 技术 | 当前状态 |
|---|---|---|
| `probe-cli.exe` | Rust + Win32 | M1–M6 诊断原型，可构建和完成无输入 lifecycle smoke |
| `inputflow-agent.exe` | 纯 Rust + `windows-sys` + Win32 | M7 Phase D 已接入安全 Named Pipe；Hook/runtime、托盘、单实例、配置热应用、capture 与事件订阅均由 agent 权威实现 |
| `InputFlow.Settings.exe` | C# + WinUI 3 + Windows App SDK | 正式页面已接入 agent 状态、Schema v4 草稿、方向规则编辑／有限预览、启停/删除、capture、保存核对、诊断和单实例 |

禁止把 Tauri、React、Node.js、npm、WebView2 或 Electron 加入 M7 构建链。WinUI 3 设置程序不得安装 Hook，也不得嵌入 agent 进程。

## 2. 2026-09-29 实际开发环境

| 项目 | 实际值与证据 |
|---|---|
| 操作系统 | 注册表 `ProductName=Windows 10 Pro`、`DisplayVersion=25H2`；实际内核/.NET 报告 `10.0.26200`，UBR 9457，x64。历史文档称 Windows 11；因此以数值 build 为复现依据，不仅依赖产品名。 |
| Rust | `rustc 1.98.1`、`cargo 1.98.1`、`stable-x86_64-pc-windows-msvc` |
| Visual Studio | Build Tools 2022 17.14.41（17.14.37710.0）；Build Tools 2026 18.10.2（18.10.12217.157）；均无完整 Visual Studio IDE |
| VS 组件 | 有 Windows SDK / Native Desktop Core；未发现 WinUI/Windows App Development workload。Phase A 使用官方 .NET CLI 模板，不依赖该 workload。 |
| Windows SDK | 已安装多个版本；本工程目标 SDK `10.0.26100.0`，最低 `10.0.17763.0` |
| .NET | SDK 10.0.401；MSBuild 18.9.11；host、Core、Windows Desktop Runtime 均为 10.0.12 x64 |
| WinUI 模板 | `Microsoft.WindowsAppSDK.WinUI.CSharp.Templates` 0.0.7-alpha；`dotnet new list winui` 可见 `WinUI Blank App` |
| Windows App SDK | NuGet `Microsoft.WindowsAppSDK` 2.5.1 |
| Windows SDK BuildTools | NuGet `Microsoft.Windows.SDK.BuildTools` 10.0.28000.2705 |
| Windows App Runtime | 官方 2.5.1 x64 安装程序退出码 0；framework-dependent smoke 随后成功启动 |
| Developer Mode | 未启用；尝试写系统设置被当前非管理员会话拒绝。未绕过 UAC，也未自动提升。 |

仓库根的 `global.json` 固定 .NET SDK 10.0.401，并只允许同一 feature band 的最新 patch。安装后新开的终端可直接使用 `dotnet`；若旧终端尚未刷新 `PATH`，可暂用 `C:\Program Files\dotnet\dotnet.exe`。

## 3. Rust workspace

在仓库根目录运行：

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p probe-cli
```

2026-10-02 Phase E 最终结果：全部通过；测试总数 151（agent 4、config 28、engine 74、protocol 12、runtime 6、windows 27、probe 0）。自动测试、无输入 lifecycle 和真实物理输入/性能证据仍按不同层级记录。

2026-10-08 Phase F 当前结果：`cargo fmt --check`、workspace test、Clippy `-D warnings` 和构建通过；测试总数 169（agent 4、config 32、engine 87、protocol 12、runtime 6、windows 28、probe 0）。新增自动覆盖四方向／阈值／偏轴／净位移、时间边界、一次命中、取消／repeat／overflow、pause／replace tombstone、physical-first、极端负坐标、注入 move、锁外输出、pre-v4 结构隔离、四方向验收配置和 125／500／1000 Hz 确定性序列。该结果不是物理鼠标证据。

运行原型：

```powershell
cargo run -p probe-cli -- --config "$env:LOCALAPPDATA\InputFlow\config.json"
```

## 4. WinUI 3 设置工程

工程位置：

```text
apps/settings-winui/
├── InputFlow.Settings.slnx
├── InputFlow.Settings/
├── InputFlow.Settings.Core/
├── InputFlow.Settings.Core.Tests/
├── InputFlow.Protocol/
└── InputFlow.Protocol.ContractTests/
```

模板生成后固定了以下边界：

- `TargetFramework=net10.0-windows10.0.26100.0`
- `TargetPlatformMinVersion=10.0.17763.0`
- Windows App SDK 2.5.1 与 Windows SDK BuildTools 10.0.28000.2705
- `WindowsPackageType=None`：unpackaged
- `WindowsAppSDKSelfContained=false`：framework-dependent
- Phase A 工程范围固定为 x64；x86/ARM64 尚未纳入支持范围

实际可重复命令：

```powershell
dotnet restore .\apps\settings-winui\InputFlow.Settings.slnx -p:Configuration=Debug
dotnet restore .\apps\settings-winui\InputFlow.Settings.slnx -p:Configuration=Release
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Debug --no-restore
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Release --no-restore
dotnet run --project .\apps\settings-winui\InputFlow.Protocol.ContractTests\InputFlow.Protocol.ContractTests.csproj -c Debug --no-build
dotnet run --project .\apps\settings-winui\InputFlow.Settings.Core.Tests\InputFlow.Settings.Core.Tests.csproj -c Debug --no-build
```

Debug 与 Release 均为 0 warning、0 error。不要给 solution 命令添加 `-p:Platform=x64` 或 `-r win-x64`：该 `.slnx` 使用默认 solution configuration，项目文件已经明确固定 `win-x64`；前述额外参数在本机 solution 构建中分别造成无效 configuration 和 `NETSDK1134`。

启动 Debug 产物：

```powershell
& .\apps\settings-winui\InputFlow.Settings\bin\Debug\net10.0-windows10.0.26100.0\win-x64\InputFlow.Settings.exe
```

Phase E 实测观察到标题为 `InputFlow 设置` 的原生窗口和真实 `phase=ready` 状态。第二个 settings 进程在 5 秒内退出并保留首个窗口；正常 `WM_CLOSE` 后首个进程 exit code 0。agent 在线、agent 先退出和 UI 单独离线三种关闭边界均不遗留 settings 进程。

## 5. 部署模式比较与决定

| 组合 | 本机证据 | 代价/限制 | Phase A 决定 |
|---|---|---|---|
| Packaged + framework-dependent | restore/build 成功；模板的 packaged 启动工具明确报 Developer Mode 未启用 | 开发启动需 Developer Mode/包注册；本轮没有管理员权限启用，故启动未验证 | 不选；保留为未来安装/分发评估项 |
| Unpackaged + self-contained | 独立构建成功；原生窗口启动并正常关闭，exit code 0 | 输出携带 .NET/Windows App SDK，体积和更新责任更大 | 已验证的离线/部署 fallback，不作为当前默认 |
| Unpackaged + framework-dependent | 安装 Windows App Runtime 2.5.1 x64 后，Debug/Release 构建成功；窗口启动并正常关闭，exit code 0 | 目标机必须具备匹配的 .NET Desktop Runtime 和 Windows App Runtime | **当前选择**；适合本地系统工具开发并由共享运行库获得服务更新 |

该选择只固定 Phase A 开发基线，不等于最终安装器方案已经完成。干净测试机的首次安装、升级、卸载和缺少运行库时的用户体验均未执行。`Package.appxmanifest` 作为官方模板源文件保留，但 `WindowsPackageType=None` 时不参与当前运行路径。

## 6. Rust agent（Phase C）

构建入口：

```powershell
cargo build -p inputflow-agent
cargo build -p inputflow-agent --release
```

Release 产品构建：

- PE subsystem 为 Windows GUI，不出现控制台窗口。
- 继续保留独立的 `probe-cli` 诊断程序。
- 不加载 WinUI、.NET 或 WebView；设置程序仍是独立进程。
- Hook、托盘和 Named Pipe 的 Win32 `unsafe` 边界集中在平台模块；协议解析、事务与运行时操作保持在安全 Rust 层。
- 正式配置与日志默认位于 `%LOCALAPPDATA%\InputFlow`；默认日志不包含逐键 identity，只有显式 `--debug-input` 才启用。
- `--run-for-ms`、`--smoke`、`--smoke-iterations` 与 `--no-tray` 是有界验证参数，不是 Phase D IPC 的替代物。

`probe-cli` 与 agent 已共用 `inputflow-runtime`；没有两份 Hook 生命周期。agent 托盘支持打开设置、pause/resume、状态文字/tooltip 和退出，tooltip/菜单直接读取 runtime 权威状态，F12 等非托盘路径也会通知刷新。Explorer 广播 `TaskbarCreated` 后重加图标。当前用户会话以 named mutex 保证单实例；第二实例请求打开设置。

Phase C 自动 smoke 示例（不含物理输入）：

```powershell
.\target\release\inputflow-agent.exe --no-tray --smoke-iterations 100 --config .\target\phase-c-smoke\config.json
.\target\release\inputflow-agent.exe --run-for-ms 12000 --config .\target\phase-c-smoke\config.json
```

第一条依次覆盖 pause/resume、应用当前配置、begin/cancel capture 和 clean shutdown。第二条建立真实托盘与 Hook 生命周期后限时退出。两者都不能替代托盘点击或物理键鼠验收。

## 7. 联合开发运行顺序（Phase E 已接线）

1. 构建并启动 `inputflow-agent.exe`；Phase D IPC 启动返回前先创建当前会话、当前进程用户可访问的版本化 Named Pipe instance。
2. C# 客户端连接后先完成 v1 handshake，再使用 status/config/validate/apply/pause/resume/stats/capture/events contract。
3. 单独启动 `InputFlow.Settings.exe`。
4. 设置程序 handshake，显示 agent 的权威状态；连接失败时只显示离线，不自行启动第二套 Hook。
5. 关闭设置窗口，确认设置进程退出；agent、托盘和规则继续运行。
6. 从托盘再次打开设置，确认单实例/激活现有窗口语义。

第 1–2 步的 agent、Named Pipe 和 C# client contract 属于 Phase D；正式页面绑定、关闭/重开、UIA、物理键录制、高对比度/缩放和长稳态资源均已完成 Phase E 验收。中文 Narrator 实际语音因本机语音环境不足保持限制，但应用侧 UIA 名称已逐项核验。

## 8. 联合构建入口

`scripts/build-windows.ps1` 调用已经验证的 Cargo、WinUI、protocol contract 和 settings core 测试；任一步失败都会返回非零。可用 `-SkipRestore` 复用已还原依赖。脚本不得：

- 自动提升权限或修改 Developer Mode。
- 静默下载未锁定工具链。
- 在测试失败后继续产生“成功”包。
- 把开发机绝对路径写入项目文件。

## 9. 验证状态

| 类别 | 截至 2026-10-08 的证据 |
|---|---|
| Rust 自动化 | fmt/test/clippy/build 通过；169/169 测试通过（agent 4、config 32、engine 87、protocol 12、runtime 6、windows 28），包含方向状态机、严格 schema 代际隔离、四方向验收配置、UIPI preflight 与同步输出重入回归 |
| 完整键盘 / Schema v4 | mapping、matcher、scan replay、v1/v2/v3 migration、v4 direction golden/strict round-trip 自动测试通过；Phase E 正式页面实测 Caps、OEM、方向、主 Enter 和 Fn 音量键录制/读回；keypad Enter/独立播放键因无硬件未实测 |
| Rust Windows smoke | Phase C agent 执行 100 次 pause/resume、apply、begin/cancel capture 后 clean quit，exit code 0、marker 清除、0 failed/0 dropped；带 2 条规则的 Phase B 物理验收仍为 1209 个观察事件、7 个完整输出批次 |
| WinUI 自动化 | restore、Debug、Release 通过，均 0 warning/0 error；protocol contract 6 项、settings core 11 项通过；v4 fixture／direction typed round-trip、capture 启动互斥/取消、规则启用绑定与既有状态机通过；Phase E 真实 agent live contract 为历史证据 |
| Phase F 物理输入 | 未执行 F-PHY-01～07：真实四方向、普通点击／拖拽、pause／replace／preview／quit、约五分钟物理 move 和资源分位仍待记录；合成频率不替代设备 polling rate |
| WinUI 生命周期 | x64 原生窗口、settings 单实例、正常关闭、agent 先退出再关 UI 均 exit code 0；UI 关闭后 agent 继续运行 |
| WinUI 功能 | 页面完成三类代表规则的创建/保存/重开/物理命中和 F12/UI/托盘同步；所有可聚焦控件 UIA Name 非空非通用，高对比度、125%/150% 与恢复 200% 缩放通过 |
| Packaged 启动 | 未验证：Developer Mode 未启用；不是代码构建失败；首版不默认要求 MSIX |
| Agent / 托盘 | Release PE subsystem=2（Windows GUI），不加载 .NET/WinUI/WebView；Active/Pause/Resume/Open Settings/Exit、F12 同步及 Explorer 真重启后的图标/菜单恢复均通过 |
| Agent 资源 | Release 五分钟 297 次 stats 查询 0 错误，p99 16.032 ms；5,458 个 callback 样本 p99 328 µs，failed/dropped 增量 0，线程 7→7、句柄 161→161。单机结果不是所有设备保证 |
| IPC | Phase D 已完成：ADR-006、1 MiB length-prefix JSON v1、当前用户 + SYSTEM DACL、handshake/request ID/错误、全部最小方法和有界事件订阅；Rust/C# golden contract 通过，真实 agent live contract（含事件与断线取消 capture）退出码 0 |
| M6/Phase B 真实输入 | M6-01～07 菜单/墓碑、四类 repeat、回放顺序、UIPI 完整/零写入、高负载、布局/修饰/光标和两秒退出均完成；partial SendInput 未实机触发，保留自动故障注入证据 |
| 部署 | 本机 runtime 安装和两种 unpackaged 启动已验证；干净机/升级/卸载未执行 |

自动测试、无输入 smoke 与真实桌面输入观察必须分栏报告。没有执行的项目写“未执行”，不能推断为通过。

## 10. 官方参考

- WinUI 3 入门：https://learn.microsoft.com/en-us/windows/apps/get-started/start-here
- Windows App SDK：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/
- Windows App SDK 下载：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads
- Unpackaged 部署：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-unpackaged-apps
- 部署概览：https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/deploy-overview
- Rust Windows bindings：https://github.com/microsoft/windows-rs

## 11. H 阶段发布构建（待实现与验证）

实施依据：`check-fix-debug-list/tag_6_InputFlow-首个Release收尾与发布任务.md`。默认优先验证 unpackaged x64 目录发布；独立 installer／MSIX／签名不默认阻塞首版，实际分发能力必须经过目标机验证。

- .NET `SelfContained` 与 `WindowsAppSDKSelfContained` 分别决定各自依赖，必须分别验证。当前项目为 framework-dependent，不提前改写状态。
- 发布针对 `InputFlow.Settings.csproj` 或明确 profile，不向 solution 加已知无效 `-r`／Platform 参数。
- 当前 Release 属性含 `PublishTrimmed=true`；首版发布 profile 默认可关闭裁剪并验证，优先目录发布，不为了体积引入单文件或 AOT。
- UI 的全部资源／DLL／运行组件随最终 publish 结果整理；Agent 的 MSVC／原生依赖也需核对，不只复制两个 exe。
- Agent 当前按自身目录找 `InputFlow.Settings.exe`，或使用 `--settings PATH`；最终布局／快捷方式必须与之匹配，且独立于工作目录。
- 正式数据继续使用 `%LOCALAPPDATA%\InputFlow`，默认不启用拦截示例；用户自启动只指向 Agent，默认关闭且可移除。
- 新 `scripts/package-release.ps1`（待新增）在完整门槛成功后组装版本包和 SHA-256；失败非零退出。

具体 publish 命令必须对当前锁定 SDK 验证后再填写，不把候选属性／模板命令标成可重复结果。

### 11.1 最终分发证据（实施后填写）

| 项目 | 当前状态／待记录内容 |
|---|---|
| ADR-009 分发模式 | 待写；编号占用时顺延 |
| 实际版本／提交／架构／支持 OS | 待记录 |
| Agent／UI publish／packaging 完整命令 | 待实现并验证 |
| .NET／Windows App SDK／原生运行依赖 | 待核对最终包 |
| 发布 profile／裁剪／ReadyToRun 等属性 | 待记录选择和结果 |
| 包路径／体积／SHA-256 | 尚未生成 |
| 干净环境／缺依赖／空格中文路径 | 未执行 |
| 自启动／升级／移除 | 未实现或未验收，以实际代码为准 |
| 最终包 RC smoke | 未执行 |
| 首个远端 release | 未发布 |

## 12. 验证时长与发布后长测

发布前只要求 tag_6 F 的短时物理移动、G-PRE 的有限输入／恢复、H 的分发和 RC 最终包 smoke。24／72 小时、长期 daily-drive 均在首版发布后，不要求在 RC 前等待。

现有五分钟采样脚本的 `DurationSeconds` 上限为 86400、固定 PID，不能直接宣称支持可靠 72 小时采样。G-POST 另行核对／扩展超时、断线重连、实例身份、分段轮转与可取消机制，保留短测工具。详见 `tag_6_InputFlow-发布后Phase-G长时间运行任务.md`。

所有证据继续分自动／故障注入／脚本／物理输入；环境限制和未执行项不得自动变成“通过”。
