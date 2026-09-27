# InputFlow

Windows 全局键盘与鼠标输入组合引擎：只暂扣可能构成已启用规则的事件，命中后消费并发送动作，失败或超时则按序尽力回放。

> 项目规划：`docs/PROJECT_PLAN.md`  
> 执行步骤：`Steps.md`  
> Windows 构建：`docs/BUILD_WINDOWS.md`  
> 当前 Codex 任务：`check-fix-debug-list/tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md`

## 当前状态

- M1–M6 Rust 原型和自动化可靠性加固已经存在；历史记录报告 workspace 92 项测试通过。
- 真实键鼠、UIPI、不同布局和高负载 Hook 存活矩阵仍有未执行项，详见 `check-fix-debug-list/M6-可靠性基线摘要.md`。
- M7 已确定为：纯 Rust + Win32 的 `inputflow-agent.exe` 常驻，C# + WinUI 3 的 `InputFlow.Settings.exe` 按需启动，以版本化 Windows Named Pipe 通信。
- 完整键盘（Caps Lock、OEM 符号、导航、数字键盘、媒体键等）进入 M7；有激活条件的鼠标方向移动进入 M8。
- 项目不使用 Tauri、React、Node.js、npm、WebView2 或 Electron。

## 产品原则

InputFlow 应像一把扳手，而不是常驻的大型桌面套件：

- agent 是唯一 Hook、托盘、规则运行时、正式配置和 IPC 服务所有者。
- 设置程序不安装 Hook、不直接写正式配置、不隐藏到托盘；关闭最后一个窗口后进程退出。
- agent 不加载 .NET、WinUI、WebView 或 JavaScript 运行时。
- Hook 热路径不执行 UI、磁盘、网络、无界分配或无界队列。
- F12（或已验证的替代组合）保留为紧急旁路。
- UI 只能提交草稿；agent 负责验证、原子保存和热应用，失败时保留旧规则。

## 已记录的开发环境

下表来自上一轮 Windows 工作区，仅作为已记录基线；M7 开工时必须重新探测并写回 `docs/BUILD_WINDOWS.md`。

| 项目 | 已记录值 |
|---|---|
| Windows | Windows 11 Pro 10.0.26200，AMD64 |
| Rust | rustc 1.98.1；`stable-x86_64-pc-windows-msvc` |
| Windows SDK | 10.0.26100.0 |
| Cargo workspace | engine / config / windows / probe-cli |
| WinUI/.NET/Windows App SDK | 尚未实测记录 |

## 当前 Rust 构建

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p probe-cli
```

M7 计划命令、Visual Studio 工作负载、WinUI 模板和部署模式见 `docs/BUILD_WINDOWS.md`。未在实际 Windows 开发机执行的命令不能写成已通过。

## 仓库结构

```text
inputflow/
├── Cargo.toml
├── README.md
├── Steps.md
├── InputFlow-项目规划.md
├── apps/
│   ├── probe-cli/            # 当前 M1–M6 原型
│   ├── inputflow-agent/      # M7 计划
│   └── settings-winui/       # M7 计划
├── crates/
│   ├── inputflow-engine/
│   ├── inputflow-config/
│   ├── inputflow-windows/
│   └── inputflow-protocol/   # M7 计划
├── docs/
│   ├── PROJECT_PLAN.md
│   ├── BUILD_WINDOWS.md
│   └── decisions/
│       ├── ADR-000-仓库结构与技术选型.md
│       ├── ADR-001-组合匹配与回放协议.md
│       ├── ADR-002-时序规则与冲突策略.md
│       ├── ADR-003-可靠性配置暂停旁路诊断恢复与性能采样.md
│       └── ADR-004-Rust常驻Agent与WinUI3设置程序.md
└── check-fix-debug-list/
    ├── M6-可靠性基线摘要.md
    └── tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md
```

计划目录尚未创建不表示功能已经完成；Codex 应按 tag_5 的阶段和进入条件逐步建立。

## 已知限制

- 当前 `Key` 模型只覆盖常用键；完整键盘必须先完成输入身份 ADR 和 Schema v2/迁移设计。
- Caps/Num/Scroll Lock 有 toggle 语义，必须实测命中、失败回放、暂停和重复，避免双重切换。
- OEM 符号键受键盘布局影响；需要同时考虑逻辑 VK、scan code、extended 和用户可读名称。
- 鼠标方向规则尚未实现；第一版必须有显式激活条件，普通移动、点击和拖拽直通。
- `SendInput` 受 UIPI、焦点和当前修饰状态影响，不能承诺 100% 原样回放。
- 进程强杀时已暂扣的历史输入无法保证恢复。
- 设置程序关闭后必须完全退出；后台资源目标要通过 CPU、工作集、线程、句柄和 Hook 分位实测验证。

