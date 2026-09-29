# InputFlow Windows 构建与运行基线

> 状态：M7 Phase A、Phase B 与 Phase C 已验证（更新于 2026-09-30）。当前已有共享 Rust runtime、常驻 Win32 agent/托盘、完整键盘身份/Schema v2，以及独立 WinUI 3 smoke。Phase D–F 尚未完成；未执行项不得写成已通过。

## 1. 目标产物与当前进度

| 产物 | 技术 | 当前状态 |
|---|---|---|
| `probe-cli.exe` | Rust + Win32 | M1–M6 诊断原型，可构建和完成无输入 lifecycle smoke |
| `inputflow-agent.exe` | 纯 Rust + `windows-sys` + Win32 | M7 Phase C 已创建；Hook/runtime、托盘、单实例、配置热应用与 capture API 已实现，IPC 待 Phase D |
| `InputFlow.Settings.exe` | C# + WinUI 3 + Windows App SDK | Phase A smoke 已创建；只显示静态说明，正式 UI/IPC 尚未接入 |

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

2026-09-29 Phase B 结果：全部通过；测试总数 113（engine 72、config 24、windows 17、probe 0）。使用 v2 golden 配置执行 no-input hook lifecycle smoke：加载 1 条 physical/logical rule，安装键鼠 Hook 后 clean quit，exit code 0、observed events 0；不能作为真实输入或性能验收。

运行原型：

```powershell
cargo run -p probe-cli -- --config "$env:LOCALAPPDATA\InputFlow\config.json"
```

## 4. WinUI 3 smoke 工程

工程位置：

```text
apps/settings-winui/
├── InputFlow.Settings.slnx
└── InputFlow.Settings/
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
dotnet restore .\apps\settings-winui\InputFlow.Settings.slnx
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Debug --no-restore
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Release --no-restore
```

Debug 与 Release 均为 0 warning、0 error。不要给 solution 命令添加 `-p:Platform=x64` 或 `-r win-x64`：该 `.slnx` 使用默认 solution configuration，项目文件已经明确固定 `win-x64`；前述额外参数在本机 solution 构建中分别造成无效 configuration 和 `NETSDK1134`。

启动 Debug 产物：

```powershell
& .\apps\settings-winui\InputFlow.Settings\bin\Debug\net10.0-windows10.0.26100.0\win-x64\InputFlow.Settings.exe
```

实测观察到标题为 `InputFlow.Settings` 的原生窗口；向窗口发送正常 `WM_CLOSE` 后，进程在等待窗口关闭的测试期限内退出，exit code 0。页面明确标注这是 Phase A build smoke；没有 Hook、配置保存、IPC 或伪造在线状态。

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
- Hook 和托盘的 Win32 `unsafe` 边界集中在平台模块；Named Pipe 要到 Phase D 才实现。
- 正式配置与日志默认位于 `%LOCALAPPDATA%\InputFlow`；默认日志不包含逐键 identity，只有显式 `--debug-input` 才启用。
- `--run-for-ms`、`--smoke`、`--smoke-iterations` 与 `--no-tray` 是有界验证参数，不是 Phase D IPC 的替代物。

`probe-cli` 与 agent 已共用 `inputflow-runtime`；没有两份 Hook 生命周期。agent 托盘支持打开设置、pause/resume、状态文字/tooltip 和退出，Explorer 广播 `TaskbarCreated` 后重加图标。当前用户会话以 named mutex 保证单实例；第二实例请求打开设置。

Phase C 自动 smoke 示例（不含物理输入）：

```powershell
.\target\release\inputflow-agent.exe --no-tray --smoke-iterations 100 --config .\target\phase-c-smoke\config.json
.\target\release\inputflow-agent.exe --run-for-ms 12000 --config .\target\phase-c-smoke\config.json
```

第一条依次覆盖 pause/resume、应用当前配置、begin/cancel capture 和 clean shutdown。第二条建立真实托盘与 Hook 生命周期后限时退出。两者都不能替代托盘点击或物理键鼠验收。

## 7. 联合开发运行顺序（Phase D/E 尚未实现）

1. 构建并启动 `inputflow-agent.exe`；Phase C 的托盘/Hook 生命周期现已可用。
2. Phase D 让 agent 建立当前用户可访问的版本化 Named Pipe。
3. 单独启动 `InputFlow.Settings.exe`。
4. 设置程序 handshake，显示 agent 的权威状态；连接失败时只显示离线，不自行启动第二套 Hook。
5. 关闭设置窗口，确认设置进程退出；agent、托盘和规则继续运行。
6. 从托盘再次打开设置，确认单实例/激活现有窗口语义。

第 1 步的 agent 生命周期属于已完成的 Phase C；Named Pipe、正式设置连接和完整联合流程仍属于 Phase D–F。Phase A smoke 不应被当作联合运行已经完成。

## 8. 联合构建入口（实现后）

M7 完成前应提供 `scripts/build-windows.ps1` 或等价入口，并调用已经验证的 Cargo 与 WinUI 命令。脚本必须失败即返回非零，且不得：

- 自动提升权限或修改 Developer Mode。
- 静默下载未锁定工具链。
- 在测试失败后继续产生“成功”包。
- 把开发机绝对路径写入项目文件。

## 9. 验证状态

| 类别 | 截至 2026-09-30 的证据 |
|---|---|
| Rust 自动化 | Phase C fmt/test/clippy/build 通过；124/124 测试通过（engine 73、config 25、windows 20、runtime 3、agent 3） |
| 完整键盘 / Schema v2 | mapping、matcher、scan replay、v1 migration、v2 golden/strict round-trip 自动测试通过；en-US/Microsoft Pinyin OEM 与 Caps 指示状态实测通过 |
| Rust Windows smoke | Phase C agent 执行 100 次 pause/resume、apply、begin/cancel capture 后 clean quit，exit code 0、marker 清除、0 failed/0 dropped；带 2 条规则的 Phase B 物理验收仍为 1209 个观察事件、7 个完整输出批次 |
| WinUI 自动化 | restore、Debug、Release 通过，均 0 warning/0 error |
| WinUI 生命周期 | unpackaged framework-dependent x64 原生窗口出现；正常关闭后进程 exit code 0 |
| WinUI 功能 | 只有静态 smoke 页面；Hook、配置、agent、IPC 均未接入 |
| Packaged 启动 | 未通过：Developer Mode 未启用；不是代码构建失败 |
| Agent / 托盘 | Release PE subsystem=2（Windows GUI），加载模块中 .NET/WinUI/WebView 命中 0；真实 tray/Hook 限时运行 exit code 0；第二实例 3/3 返回 0 并分别启动设置；用户已人工通过 Active/Pause/Resume/Open Settings/Exit，Explorer 恢复代码已实现但真实 Explorer 重启待执行 |
| Agent 资源 | Release 空闲约 10.3 MiB working set、1.6 MiB private bytes、6 threads、143 handles；1000 次 pause/resume 后约 10.0 MiB/1.7 MiB、7 threads、145 handles，3 次打开设置请求并关闭后 agent 指标增长均为 0。单机短样本，不是最终性能门槛 |
| IPC | Phase D 未实现、未执行 |
| M6/Phase B 真实输入 | Phase B 的 en-US/Microsoft Pinyin OEM、Caps 失败/命中/指示灯与 `F12` pending 恢复通过；M6 菜单/墓碑、UIPI、100k/高负载等仍未执行 |
| 部署 | 本机 runtime 安装和两种 unpackaged 启动已验证；干净机/升级/卸载未执行 |

自动测试、无输入 smoke 与真实桌面输入观察必须分栏报告。没有执行的项目写“未执行”，不能推断为通过。

## 10. 官方参考

- WinUI 3 入门：https://learn.microsoft.com/en-us/windows/apps/get-started/start-here
- Windows App SDK：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/
- Windows App SDK 下载：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads
- Unpackaged 部署：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deploy-unpackaged-apps
- 部署概览：https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/deploy-overview
- Rust Windows bindings：https://github.com/microsoft/windows-rs
