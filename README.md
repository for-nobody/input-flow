# InputFlow

InputFlow 是 Windows 全局键盘与鼠标输入组合引擎。它只暂扣可能构成已启用规则的事件；
命中后消费输入并发送动作，失败或超时则按序尽力回放。

## 当前状态

M1～M7、Phase F／M8 和发布前 G-PRE 已完成。鼠标四方向、Schema v4、IPC、WinUI、真实方向
与边界、约 16 分钟混合输入、UI／Pipe／配置／恢复及 pause／replace／正常退出回归均已收口。
分发阶段 H 已完成，包括本机工程包、生命周期、真实重启登录与全新 Windows x64 环境验收。项目已
`v0.9.0` RC 已完成：版本、MIT License、固定包、许可附件、说明、校验和，以及精确 ZIP 的自动、
桌面和物理输入 smoke 均已通过。首个 release 尚未发布；发布前将按用户后续要求增加英语支持补丁，
并重新生成包和复验受影响路径。24／72 小时长测安排在首版发布后。

详细状态和下一步以
[`docs/status/CURRENT_STATUS.md`](docs/status/CURRENT_STATUS.md) 为唯一权威。

## 文档入口

- [文档索引](docs/README.md)
- [当前执行顺序](docs/planning/IMPLEMENTATION_STEPS.md)
- [项目规划](docs/planning/PROJECT_PLAN.md)
- [首版发布路线](docs/planning/RELEASE_ROADMAP.md)
- [当前首版发布任务（当前执行 RC）](docs/tasks/FIRST_RELEASE.md)
- [v0.9.0 发布说明草稿](docs/releases/V0.9.0.md)
- [RC 执行记录](docs/records/FIRST_RELEASE_RC_EXECUTION.md)
- [G-PRE 完成记录](docs/archive/first-release/G_PRE.md)
- [Phase F 完成记录](docs/archive/phase-f/PHASE_F.md)
- [Windows 构建指南](docs/guides/BUILD_WINDOWS.md)
- [文档存储与生命周期规则](docs/governance/DOCUMENTATION_POLICY.md)

## 产品原则

- `inputflow-agent.exe` 是唯一的 Hook、托盘、规则运行时、正式配置和 IPC 服务所有者。
- `InputFlow.Settings.exe` 是按需启动的 C# + WinUI 3 设置程序；关闭最后一个窗口后进程退出。
- 设置程序不安装 Hook、不直接写正式配置；Agent 负责验证、原子保存和热应用。
- Agent 不加载 .NET、WinUI、WebView 或 JavaScript 运行时。
- Hook 热路径不执行 UI、磁盘、网络、无界分配、无界队列或等待 UI。
- F12 保留为紧急旁路；输入录制不能吞掉它。
- 项目不使用 Tauri、React、Node.js、npm、WebView2 或 Electron。

## 已实现能力

- Hold、KeyChord、Key+MouseButton 和 Hold+MouseButton 触发器。
- logical／physical 键身份、完整具名键、scan-code 回放和布局相关显示名。
- 持久化规则启停、配置验证、原子保存、恢复和运行时热替换。
- Rust/Win32 常驻 Agent、托盘、单实例和 Explorer 托盘恢复。
- 版本化、有限帧、当前用户 ACL 的 Windows Named Pipe。
- WinUI 规则编辑、录制、保存、连接协调、诊断和可访问性支持。
- Schema v4 鼠标四方向规则：键盘激活、净位移阈值、偏轴容差、超时和一次命中；普通 move
  始终直通。Schema v1／v2／v3 保持可读并确定迁移。

## 快速构建

Rust workspace：

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p inputflow-agent --release
```

完整 Rust + C# 验证链：

```powershell
./scripts/build-windows.ps1
```

具体工具链、restore、WinUI Debug／Release 命令和运行依赖见
[`docs/guides/BUILD_WINDOWS.md`](docs/guides/BUILD_WINDOWS.md)。

## 仓库结构

```text
input-flow/
├── README.md                    # 根目录唯一 Markdown；项目摘要
├── apps/
│   ├── probe-cli/               # 共用 runtime 的诊断控制台
│   ├── inputflow-agent/         # Rust/Win32 常驻 Agent
│   └── settings-winui/          # WinUI 设置、状态核心、协议客户端和 contract runners
├── crates/
│   ├── inputflow-engine/
│   ├── inputflow-config/
│   ├── inputflow-runtime/
│   ├── inputflow-windows/
│   └── inputflow-protocol/
├── docs/
│   ├── README.md                # 文档索引
│   ├── governance/              # 文档规则
│   ├── planning/                # 稳定规划与路线
│   ├── status/                  # 当前进度唯一权威
│   ├── tasks/                   # 未完成任务
│   ├── records/                 # 当前阶段证据
│   ├── guides/                  # 构建与验收指南
│   ├── releases/                # 版本对应的用户发布说明
│   ├── reference/               # 术语、研究和 fixture 契约
│   ├── decisions/               # ADR
│   └── archive/                 # 已完成任务和历史快照
├── fixtures/                    # 配置与协议 golden JSON
└── scripts/                     # 构建和 Windows 验收工具
```

## 已知边界

- 鼠标方向已完成真实四方向、普通拖拽和约五分钟物理 move 验收；本机只有一个显示器，真实
  跨屏／跨 DPI／热插拔以及实际 polling rate 仍未验证。
- G-PRE 的 941 个样本保留 stats RTT p99 151.274 ms／max 1064.207 ms 的调度尖峰；callback
  p99 0.340 ms／max 17.265 ms，且没有输出失败、丢弃或 Hook 中断；发布后长测继续观察。
- `SendInput` 受 UIPI、焦点和当前修饰状态影响，不能承诺所有目标中 100% 原样回放。
- 进程被强杀时，已暂扣的历史输入无法保证恢复。
- keypad Enter、独立播放键、中文 Narrator 语音和 partial `SendInput` 仍受当前硬件／环境限制。
- H 工程包、本机自启动、真实登录、升级／移除模拟和全新 Windows x64 环境验收已完成。
- 首版定位公开测试版本；发布前不要求 24／72 小时长测，也不宣称已经证明长期稳定。

## 许可证

InputFlow 采用 [MIT License](LICENSE)。第三方组件及其许可证见
[`THIRD-PARTY-NOTICES.txt`](THIRD-PARTY-NOTICES.txt)；发布包还包含锁定依赖的完整许可文件。
