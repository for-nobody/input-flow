# ADR-004：纯 Rust 常驻 Agent 与 WinUI 3 设置程序

- 状态：已接受（Accepted）
- 日期：2026-09-27
- 涉及模块：`apps/inputflow-agent`、`apps/settings-winui`、`inputflow-runtime`、`inputflow-windows`、`inputflow-config`、`inputflow-protocol`
- 取代：ADR-000 中“Tauri 2 + React + TypeScript”桌面 GUI 决策；ADR-000 的其余决定继续有效
- 后续：ADR-008 已将当前 handshake 门槛升级为 Schema v4／`config_v4`，不改变本文的 agent／UI 所有权和双连接协调结论

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

#### Phase A 部署决定（2026-09-29）

本机比较后，初始开发模式固定为 **unpackaged + framework-dependent**：

- unpackaged + framework-dependent 在安装 Windows App Runtime 2.5.1 x64 后完成 Debug/Release 构建、原生窗口启动和正常关闭，进程退出码 0；共享运行库可独立获得服务更新，但部署必须明确安装匹配的 .NET Desktop Runtime 和 Windows App Runtime。
- unpackaged + self-contained 也完成构建、启动和退出，作为需要捆绑运行库时的已验证 fallback；代价是更大的输出与由应用承担运行库更新。
- packaged + framework-dependent 能构建，但开发启动要求 Developer Mode/包注册；当前非管理员会话无法启用 Developer Mode，因此 packaged 启动仍是未验证项。

该决定只覆盖 Phase A 开发基线。最终安装器、干净机首次运行、升级和卸载仍需单独验收；不得由本次 smoke 推断已经具备产品部署能力。详细版本、命令和输出位置见 `docs/BUILD_WINDOWS.md`。

### 5. Phase C runtime 落地（2026-09-30）

- `inputflow-runtime` 是 probe 与产品 agent 共用的唯一生命周期编排层；薄入口不得复制 Hook 安装、ready、暂停、退出或配置事务。
- 进程唯一 Win32 callback bridge 仍由 `OnceLock` 承载，但 cell 内保存的是可替换值。callback 只在 Hook 线程存活时读取这些进程期 cell，不保存调用者借用，因此不存在跨 callback 的悬垂引用；重复 start 前必须确认旧 Hook 线程已经退出并重置运行状态。
- 所有影响 matcher 时序状态的 pause/resume/rule replacement/capture begin/cancel 都发到 Hook owner 串行执行。规则替换先冲刷 pending，进入 bypass；冲刷不完整即停止替换并报告恢复需求。成功替换保留旧规则已经产生的 consumed release tombstone，再按替换前状态决定是否恢复 interception。
- `apply_config` 固定为“权威验证与编译 → 原子保存 → Hook-owner 替换”。Hook 控制结果必须区分“未开始且已取消”、“确定失败”与“已开始但超时，结果待核对”。前两者才立即尝试把旧配置原子写回；结果待定时保留已落盘的 draft，阻止后续 apply，并在后台等待原请求终态。延迟成功则把 `current_config` 与元数据对齐到 draft，延迟失败才回滚旧配置；如果永远无法确定终态，保留 draft 并要求重启 agent 从磁盘确定性恢复。`ApplyReport` 必须包含 outcome、request id、save/runtime/rollback、cleanup warning 和 `recovery_required`，不能以模糊布尔值隐藏部分提交。
- capture 是有界且一次一个的 immutable observation；它不暂停 matcher、不预消费输入、不改变现有规则，排除本程序注入与紧急旁路键，并在取消、超时或 shutdown 时给出终态。
- agent 使用 Win32 notification icon，托盘 pause/resume 复用上述控制路径；tooltip 和菜单直接读取 runtime 的权威 suspended 状态，F12、托盘、输出失败与 overflow 状态转换都通过同一通知路径刷新展示，不得另维护托盘布尔值。`TaskbarCreated` 会重加图标。当前用户会话以 named mutex 保证单实例，第二实例请求启动设置程序。Release PE subsystem 为 Windows GUI，默认日志只记录聚合/生命周期信息；逐输入 identity 需显式 `--debug-input`。

### 6. Phase E 设置程序落地（2026-09-30）

- 设置程序使用应用级 control 与 event 两条独立连接；只有 handshake 同时满足 protocol v1、Schema v3 和 `config_v3` capability 才进入 connected。事件 ID 缺口、事件流断开或重连都会通过 control 连接重新读取权威 status/config，不用 UI 缓存猜测 agent 状态。
- `get_config` 的正式快照与深拷贝 draft 严格分离。新增、编辑、删除、启停和紧急键修改只改变 draft；保存按“外部变化提示 → agent validate → 单次 apply → get_config 读回核对”执行。mutation 超时不自动重试，界面保留草稿并显示结果未知/核对状态。
- 正式页面采用 `NavigationView`，包含快捷规则、设置与关于；规则编辑覆盖当前后端支持的 key chord、key+mouse、hold、hold+mouse trigger 以及可变长 key-chord action。动态键名只在非 Hook 的 UI 路径调用 Windows 键盘 API，落盘仍使用稳定 identity。
- 录制复用 agent capture session；每个字段至多一个活动 session，先建立事件流再 begin，Esc/按钮取消后立即使本地 generation 失效，迟到或旧 session 事件不能写入字段。录制默认 logical，只有存在非零 scan 时允许选择 physical。
- settings 进程用当前会话 mutex 保证单实例；第二实例恢复并前置已有窗口。最后一个窗口关闭后有界清理连接并退出，即使 agent 已先停止也不得遗留隐藏 UI 进程。关闭脏草稿时明确提供保存、丢弃或继续编辑。
- UI 不把未接线能力伪装成产品开关：登录启动、系统辅助功能、配置恢复和未来鼠标方向不进入正式可操作页面；统计缺样本显示“暂无样本”，不会补造零值。

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
- `OnceLock` bridge 仍是进程全局，因此同一进程同一时刻只允许一个 runtime/Hook owner；Phase C 支持安全值替换与 shutdown 后重启，但不把它伪装成多 runtime 并存模型。

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
