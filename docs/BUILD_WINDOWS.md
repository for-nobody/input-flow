# InputFlow Windows 构建与运行基线

> 状态：M7 前置构建说明（2026-09-27）。当前仓库只包含 M1–M6 Rust workspace；`inputflow-agent` 和 WinUI 3 设置工程尚未创建。未执行的命令必须标为“计划”，不能写成已通过。

## 1. 目标产物

| 产物 | 技术 | 生命周期 |
|---|---|---|
| `probe-cli.exe` | Rust + Win32 | M1–M6 诊断原型，手动运行 |
| `inputflow-agent.exe` | 纯 Rust + `windows-sys` + Win32 | M7 产品常驻进程；唯一 Hook/托盘/IPC/config 所有者 |
| `InputFlow.Settings.exe` | C# + WinUI 3 + Windows App SDK | M7 按需设置程序；关闭最后窗口后退出 |

禁止把 Tauri、React、Node.js、npm 或 WebView2 加入 M7 构建链。WinUI 3 设置程序也不得被嵌入 agent 进程。

## 2. 当前已记录环境

以下数据来自 M6 Windows 记录，只证明现有 Rust 原型的环境：

| 项目 | 已记录值 |
|---|---|
| 操作系统 | Windows 11 Pro 10.0.26200，AMD64 |
| Rust | 1.98.1，`stable-x86_64-pc-windows-msvc` |
| Windows SDK | 10.0.26100.0 |
| `windows-sys` | 0.61.2 |
| 完整 MSVC C++ workload | 当时未安装；Rust 原型通过自包含链接构建 |

M7 开始前，Codex 必须在实际 Windows 开发机重新记录：

```powershell
rustc --version --verbose
cargo --version
dotnet --info
git --version
```

并通过 Visual Studio Installer 或 `vswhere` 记录：

- Visual Studio 版本与 edition。
- 已安装的 WinUI/Windows 应用开发工作负载和组件。
- Windows SDK 版本。
- Windows App SDK/WinUI 3 项目模板是否可用。
- .NET SDK 与目标 TFM。

不要猜测组件显示名或版本；把实际输出摘要写入本文件和 README。

## 3. 当前 Rust workspace

在仓库根目录运行：

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p probe-cli
```

当前历史记录为 92 项自动测试通过，但新的执行者必须亲自运行并记录新的结果。Linux/容器编译不能替代 Windows Hook、`SendInput` 和文件替换实测。

运行原型：

```powershell
cargo run -p probe-cli -- --config "$env:LOCALAPPDATA\InputFlow\config.json"
```

## 4. 计划中的 Rust agent 构建

工程创建后，预期入口：

```powershell
cargo build -p inputflow-agent
cargo build -p inputflow-agent --release
```

实际包名、可执行文件名和命令应以生成的 `Cargo.toml` 为准并回写。Release 产品构建应：

- 使用 Windows GUI subsystem，不出现控制台窗口。
- 继续保留可独立运行的 `probe-cli` 诊断程序。
- 不静态链接或动态加载 WinUI/.NET/WebView。
- 将 Hook、托盘和 Named Pipe 的 Win32 `unsafe` 边界集中在平台模块。

不要在实现 agent 前机械复制 `probe-cli/main.rs`。先抽取可测试的 runtime/lifecycle 层，再让 probe 和 agent 复用。

## 5. 计划中的 WinUI 3 设置工程

目标目录：

```text
apps/settings-winui/
├── InputFlow.Settings.sln
└── InputFlow.Settings/
```

工程应使用 C# + WinUI 3 + Windows App SDK。创建前先用当前官方模板生成最小空应用并完成启动/关闭 smoke，再固定：

- project SDK 和 TargetFramework。
- Windows App SDK 版本。
- packaged 或 unpackaged。
- framework-dependent 或 self-contained。
- x64/ARM64 支持范围。

初始倾向是方便本地系统工具开发的 unpackaged、framework-dependent 方案，但这只是待验证起点；若模板、启动、部署或自启动需求给出反证，应先记录比较并更新 ADR。

创建后记录并验证实际命令，例如：

```powershell
dotnet restore .\apps\settings-winui\InputFlow.Settings.sln
dotnet build .\apps\settings-winui\InputFlow.Settings.sln -c Debug
dotnet build .\apps\settings-winui\InputFlow.Settings.sln -c Release
```

如果实际 WinUI 项目必须用 `msbuild` 或 Visual Studio 才能可靠构建，应记录真实命令，不要为了文档一致性保留失败的 `dotnet build` 示例。

## 6. 联合开发运行顺序

1. 构建并启动 `inputflow-agent.exe`。
2. agent 建立当前用户可访问的版本化 Named Pipe，并开始托盘/Hook 生命周期。
3. 单独启动 `InputFlow.Settings.exe`。
4. 设置程序 handshake，显示 agent 的权威状态；连接失败时只显示离线，不自行启动第二套 Hook。
5. 关闭设置窗口，确认设置进程退出；agent、托盘和规则继续运行。
6. 从托盘再次打开设置，确认单实例/激活现有窗口语义。

## 7. 联合构建入口（实现后）

M7 完成前应提供一个 PowerShell 构建入口，例如 `scripts/build-windows.ps1`，但脚本必须调用已验证的 Cargo 和 WinUI 构建命令，且失败立即返回非零退出码。

建议支持：

```powershell
.\scripts\build-windows.ps1 -Configuration Debug
.\scripts\build-windows.ps1 -Configuration Release
```

脚本不应：

- 自动安装 Visual Studio、SDK 或提升权限。
- 静默下载未锁定工具链。
- 在测试失败后继续产生“成功”包。
- 把开发机绝对路径写入项目文件。

## 8. 必须分别记录的验证

| 类别 | 证据 |
|---|---|
| Rust 自动化 | fmt/test/clippy/build 原始摘要 |
| WinUI 自动化 | restore/build、最小启动/关闭、UI 测试结果 |
| IPC | 版本不匹配、超时、断线、重连、错误 DTO、当前用户 ACL |
| 生命周期 | agent 单实例、设置单实例、反复开关设置、干净退出 |
| 资源 | 设置关闭后 UI 进程消失；agent 空闲 CPU/内存/句柄/线程基线 |
| 输入 | 真实键鼠、UIPI、布局、Caps Lock/OEM 键、鼠标高频移动 |
| 部署 | 干净测试机安装/首次运行/卸载；运行库缺失时的真实表现 |

自动测试、无输入 smoke 与真实桌面观察必须分栏报告。没有执行的项目写“未执行”，不能推断为通过。

## 9. 官方参考

- WinUI 3 入门：https://learn.microsoft.com/en-us/windows/apps/winui/winui3/
- Windows App SDK：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/
- Windows App SDK 部署：https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/deploy-overview
- Visual Studio Windows 应用工具：https://learn.microsoft.com/en-us/windows/apps/dev-tools/visual-studio
- Rust Windows bindings：https://github.com/microsoft/windows-rs
