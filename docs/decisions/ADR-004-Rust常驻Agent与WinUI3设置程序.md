# ADR-004：纯 Rust 常驻 Agent 与 WinUI 3 设置程序

- 状态：已接受（Accepted）
- 日期：2026-09-27
- 涉及模块：`apps/inputflow-agent`、`apps/settings-winui`、`inputflow-windows`、`inputflow-config`、未来 IPC 协议模块
- 取代：ADR-000 中“Tauri 2 + React + TypeScript”桌面 GUI 决策；ADR-000 的其余决定继续有效

## 背景（Context）

InputFlow 是输入基础设施，不是需要长期显示内容的普通桌面应用。它必须持续拥有低级键鼠 Hook，但用户通常只在创建规则、查看诊断或暂停时打开设置。

产品目标是“像一把扳手”：平时只有很小的后台工具存在，需要配置时才打开完整界面。用户还要求界面尽可能符合 Windows 原生外观和交互，不希望 WebView、JavaScript 或完整 UI 运行时一直随 Hook 常驻。

现有代码已经把纯规则引擎、配置和 Win32 平台层拆成 Rust crate；M6 原型由 `probe-cli` 编排。M7 需要在不破坏 Hook 可靠性边界的前提下加入托盘、可视化规则管理和输入录制。

## 候选方案（Options）

### 方案 A：单进程 Tauri 2 + React

- Rust 与前端命令连接简单，可直接复用 crate。
- 规则编辑器开发速度快。
- Windows 内部界面由 WebView2 渲染，不是真正的 WinUI 控件。
- 若关闭窗口后隐藏到托盘，WebView/前端运行环境会与 Hook 一起长期存在。
- UI 生命周期与输入引擎生命周期绑定。

### 方案 B：Rust agent + 独立 Tauri 设置程序

- 后台 agent 可以保持轻量，设置程序可退出。
- 仍需要跨进程 IPC，因此 Tauri“前端直接调用同进程 Rust”的主要优势减少。
- 内部控件仍为 Web UI。

### 方案 C：单进程 WinUI 3 应用

- 原生 Windows UI。
- 关闭/隐藏窗口后仍可能让 .NET/WinUI 与 Hook 同进程常驻。
- UI 故障、升级和生命周期与输入引擎耦合。

### 方案 D：纯 Rust + Win32 agent，独立 C# + WinUI 3 设置程序

- 只有 Rust/Win32 agent 常驻，不加载 WebView、Node.js、.NET 或 WinUI。
- 设置程序按需启动，关闭最后一个设置窗口后完全退出。
- WinUI 3 提供当前 Windows 原生控件、无障碍、高 DPI、主题和键盘导航基础。
- 需要设计版本化 IPC 和跨语言 DTO；构建链比单一 Cargo workspace 更复杂。

## 最终决策（Decision）

选择方案 D。

### 1. 运行时进程

`inputflow-agent.exe`：

- 使用 Rust stable、`windows-sys` 和公开 Win32 API。
- 是唯一低级键盘/鼠标 Hook 所有者。
- 拥有 matcher、正式配置、原子保存/恢复、崩溃标记、诊断、托盘和 IPC server。
- 通过 Win32 `Shell_NotifyIcon` 提供打开设置、暂停/恢复、状态和退出。
- 只有 agent 可配置为随用户登录启动。
- Release 构建不显示控制台窗口；`probe-cli` 继续作为诊断/回归工具，不被伪装成产品 agent。

`InputFlow.Settings.exe`：

- 使用 C#、WinUI 3 和 Windows App SDK。
- 只在用户从托盘或快捷方式打开设置时启动。
- 通过 Named Pipe 读取状态、验证/应用规则、控制暂停、进行显式输入录制和读取诊断。
- 不安装 Hook、不创建第二个 matcher、不直接写正式配置文件。
- 关闭最后一个设置窗口后退出进程；不隐藏到托盘，不随系统启动。

### 2. IPC

- 使用 Windows Named Pipe；只允许当前交互用户访问，具体 ACL 必须在实现 ADR 中写明并测试。
- 协议必须有版本、请求 ID、明确的 request/response/error、超时和断线语义。
- UI 不高频轮询；状态变化和录制候选优先使用有界事件推送，必要查询应节流。
- Hook 回调不得等待 IPC、UI、磁盘或网络。
- agent 不可用时 UI 显示“未连接”，不得在本地假装规则已应用。
- `apply_config` 必须由 agent 统一完成校验、持久化和 Hook 线程安全替换；操作结果需说明哪些阶段已提交，不能只返回模糊布尔值。

### 3. UI 信息架构

设置程序至少包含：

- 状态：运行/暂停、Hook 健康、启用规则数、最近错误。
- 规则：列表、创建、编辑、启用/停用、删除、冲突与延迟提示。
- 诊断：匿名化错误、输出失败、性能摘要和可导出信息。
- 设置：紧急键、登录启动、诊断级别及配置恢复入口。
- 关于：版本、支持边界和第三方许可。

优先使用 WinUI 3 原生控件，如 `NavigationView`、`CommandBar`、`ToggleSwitch`、`InfoBar`、`ContentDialog` 和 `NumberBox`。视觉效果不得先于键盘导航、高对比度、缩放、屏幕阅读器名称和错误可理解性。

### 4. 构建与部署

- Rust 部分继续由 Cargo 构建和测试。
- WinUI 3 部分由受支持的 Visual Studio/.NET/Windows App SDK 工具链构建。
- 初始 packaged/unpackaged、framework-dependent/self-contained 组合在 Windows 开发机实测后固定；不得在没有构建/安装证据时宣称“单文件”“免运行库”或具体体积。
- 项目不再需要 Tauri、React、Node.js、npm 或 WebView2 作为应用技术栈。
- 最终应提供一个可重复的联合构建入口，但不允许为了统一命令而把 Hook 放进 UI 进程。

## 关键不变量（Invariants）

1. agent 是唯一 Hook 和输入状态所有者。
2. UI 退出、崩溃、升级或断线不得终止 agent 或改变正在运行的规则。
3. 暂停、恢复、替换规则和退出继续由 Hook 消息线程串行化；IPC 不能绕开 ADR-003 的顺序保证。
4. UI 不直接写正式配置，也不复制权威规则校验逻辑。
5. 默认日志不记录实际键入文本、窗口标题或连续鼠标坐标；输入录制必须由用户显式开始、有超时且可取消。
6. 关闭设置后不保留隐藏 WinUI 窗口或后台 UI 进程。

## 后果（Consequences）

### 正面

- 常驻路径只包含现有 Rust/Win32 技术栈，符合轻量工具目标。
- 设置界面使用真正的 Windows 原生控件。
- UI 生命周期与输入可靠性隔离。
- 将来可以独立升级设置程序或替换 UI，而不重写 engine。

### 成本与风险

- 需要维护 Rust 与 C# 两套构建环境。
- 需要设计并测试跨进程协议、DTO 兼容和 ACL。
- WinUI 3 设置程序打开时仍有其运行时成本；本 ADR 的目标是避免它长期常驻，而不是声称打开时零开销。
- 当前 `inputflow-windows` 使用进程全局 `OnceLock` 安装 matcher/紧急键，尚不支持产品级热更新与重复生命周期；M7 必须先封装 agent runtime 和 Hook 线程控制命令。

## 验收证据（Acceptance Evidence）

- 设置程序关闭后，进程列表中不再存在 `InputFlow.Settings.exe`，agent 和规则继续工作。
- agent 进程不加载 .NET、WinUI 或 WebView 运行时模块。
- UI 断线/崩溃期间普通输入和已启用规则行为不变。
- pause/resume/apply_config 具有请求 ID、超时和完成报告，并保留 M6 的顺序/墓碑语义。
- Windows 构建、启动、关闭、重复打开设置和升级场景有可复现记录。

## 官方依据

- WinUI 3：https://learn.microsoft.com/en-us/windows/apps/winui/winui3/
- Windows App SDK：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/
- Windows App SDK 部署：https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/deploy-overview
- Named Pipes：https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes
- Shell_NotifyIcon：https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw
