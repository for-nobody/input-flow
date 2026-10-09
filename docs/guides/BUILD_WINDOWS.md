# InputFlow Windows 构建与运行基线

> 文档类型：构建与运行指南
>
> 当前进度和最新验证结果：[`../status/CURRENT_STATUS.md`](../status/CURRENT_STATUS.md)

## 0. 适用范围

本文记录可重复的开发构建、运行依赖和 H／RC 发布构建约束，不维护阶段状态或测试计数。历史
环境和命令用于复现当前工程；执行新验证时把实际结果写入对应 `docs/records/` 文件。

Phase A–E 的普通开发构建继续采用 framework-dependent 基线；H 的正式发布 profile 已实现 .NET 与
Windows App SDK 双 self-contained 目录发布。开发构建成功仍不代表自启动、升级／移除或干净环境已经
验收，当前事实以 H 记录为准。

## 1. 目标产物与职责

| 产物 | 技术 | 职责 |
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

2026-10-08 Phase F 最终结果：`cargo fmt --check`、workspace test、Clippy `-D warnings` 和构建通过；测试总数 171（agent 4、config 32、engine 87、protocol 12、runtime 6、windows 30、probe 0）。新增自动覆盖四方向／阈值／偏轴／净位移、时间边界、一次命中、取消／repeat／overflow、pause／replace tombstone、physical-first、极端负坐标、同源 Hook 坐标、repeat 遥测、注入 move、锁外输出、pre-v4 结构隔离、四方向验收配置和 125／500／1000 Hz 确定性序列。真实物理证据另见 [`../archive/phase-f/PHASE_F.md`](../archive/phase-f/PHASE_F.md)。

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
- 普通开发构建 `WindowsAppSDKSelfContained=false`；发布 profile 单独覆盖为 `true`
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

脚本先通过 `vswhere` 选择实际具备 x64 MSVC import libraries 的 Visual Studio／Build Tools 安装并
导入 `VsDevCmd.bat` 环境。不能依赖 PATH 中碰巧排在前面的不完整安装；找不到 `msvcrt.lib` 时必须
在运行测试前失败。

- 自动提升权限或修改 Developer Mode。
- 静默下载未锁定工具链。
- 在测试失败后继续产生“成功”包。
- 把开发机绝对路径写入项目文件。

## 9. 验证记录位置

- 当前阶段的实际命令、测试数量和结果写入 [`../records/`](../records/)。
- 当前跨阶段结论写入 [`../status/CURRENT_STATUS.md`](../status/CURRENT_STATUS.md)。
- M6／M7 的已完成 Windows、资源和 UI 证据保存在 [`../archive/m6/`](../archive/m6/) 和
  [`../archive/m7/`](../archive/m7/)。
- 自动测试、无输入 smoke 与真实桌面输入观察必须分栏报告。没有执行的项目写“未执行”，不能
  根据代码、进程存活或历史结果推断为通过。

## 10. 官方参考

- WinUI 3 入门：https://learn.microsoft.com/en-us/windows/apps/get-started/start-here
- Windows App SDK：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/
- Windows App SDK 下载：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads
- Unpackaged 部署：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-unpackaged-apps
- 部署概览：https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/deploy-overview
- Unpackaged WinUI publish PRI 问题：https://github.com/microsoft/WindowsAppSDK/issues/6720
- Visual C++ 文件再分发：https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
- Rust Windows bindings：https://github.com/microsoft/windows-rs

## 11. H／RC 发布构建

实施依据：[`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md) 和
[`../decisions/ADR-009-首版分发与用户生命周期.md`](../decisions/ADR-009-首版分发与用户生命周期.md)。
首版采用 unpackaged Windows x64 便携目录；独立 installer／MSIX／签名不属于默认首版范围。

统一入口（默认先执行第 8 节完整门槛）：

```powershell
.\scripts\package-release.ps1
```

已完成 restore 时可用 `-SkipRestore`；替换已存在的同版本输出必须显式用 `-Force`。只有调用者刚刚
完成同一工作树的完整门槛时才使用 `-SkipBuild`。输出位于 `target\distribution`，包含版本目录、zip
和 `SHA256SUMS.txt`；输出根必须位于仓库内，已有输出默认拒绝覆盖。

RC 必须从已经固定的干净提交运行，并增加 `-RequireClean`；脚本会在构建前后检查 Git 状态，同时核对
Rust workspace、Agent handshake、Settings assembly、协议客户端和两个 Windows manifest 的版本面：

```powershell
.\scripts\package-release.ps1 -RequireClean -Force
```

Settings 的实际 publish 命令由脚本针对具体项目和 profile 执行：

```powershell
dotnet publish .\apps\settings-winui\InputFlow.Settings\InputFlow.Settings.csproj `
  -c Release -r win-x64 -p:PublishProfile=win-x64
```

`win-x64.pubxml` 固定 `PublishSelfContained=true`、`WindowsAppSDKSelfContained=true`，并关闭 trim、
single-file、AOT 和 ReadyToRun。Windows App SDK 2.5.1 当前会生成项目 PRI 但遗漏出 publish 目录；
缺少 `InputFlow.Settings.pri` 会导致 Settings 延迟以 `0xc000027b` 崩溃。项目中的
`AddInputFlowProjectPriToPublish` target 仅把已生成 PRI 加入发布清单，且在 SDK 自行修复后可移除。

发布目录必须保留全部 UI DLL、XBF、PRI 和运行组件，Agent 与 `InputFlow.Settings.exe` 同目录。Agent
PE x64 imports 已核对：除 Windows 系统 API 外需要中央安装的 `VCRUNTIME140.dll`，因此目标机必须安装
Microsoft Visual C++ Redistributable 2015–2022 x64；不得从开发机或 System32 复制该 DLL 入包。

正式数据继续位于 `%LOCALAPPDATA%\InputFlow`，包内不含配置、日志或 PDB。Release Agent 会重映射
Rust 诊断源路径，Release WinUI 项目关闭 CodeView/PDB 路径；打包器还会扫描包内文件并拒绝仓库或
构建用户目录的绝对路径。Startup 脚本只管理当前用户的 Agent 快捷方式且默认关闭。项目
`LICENSE.txt`、第三方清单、版本发布说明，以及锁定 Cargo、NuGet、.NET 输入随带的实际法律文件会
一同打包。H 的分发证据记录在
[`../records/FIRST_RELEASE_H_EXECUTION.md`](../records/FIRST_RELEASE_H_EXECUTION.md)，RC 的固定提交、
包大小、SHA-256 和 smoke 记录在
[`../records/FIRST_RELEASE_RC_EXECUTION.md`](../records/FIRST_RELEASE_RC_EXECUTION.md)；本指南不固化某次运行结果。

## 12. 验证时长与发布后长测

发布前只要求 Phase F 的短时物理移动、G-PRE 的有限输入／恢复、H 的分发和 RC 最终包 smoke。24／72 小时、长期 daily-drive 均在首版发布后，不要求在 RC 前等待。

现有五分钟采样脚本的 `DurationSeconds` 上限为 86400、固定 PID，不能直接宣称支持可靠 72 小时采样。G-POST 另行核对／扩展超时、断线重连、实例身份、分段轮转与可取消机制，保留短测工具。详见 [`../tasks/POST_RELEASE_SOAK.md`](../tasks/POST_RELEASE_SOAK.md)。

所有证据继续分自动／故障注入／脚本／物理输入；环境限制和未执行项不得自动变成“通过”。
