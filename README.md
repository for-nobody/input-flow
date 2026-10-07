# InputFlow

Windows 全局键盘与鼠标输入组合引擎：只暂扣可能构成已启用规则的事件，命中后消费并发送动作，失败或超时则按序尽力回放。

> 项目规划：`docs/PROJECT_PLAN.md`  
> 执行步骤：`Steps.md`  
> Windows 构建：`docs/BUILD_WINDOWS.md`  
> 当前 Codex 任务：`check-fix-debug-list/tag_6_InputFlow-Phase-F-鼠标方向实施任务.md`  
> 首版发布路线：`docs/RELEASE_ROADMAP.md`  
> 文档交接入口：`00_InputFlow-文档交接与执行入口.md`

## 当前状态

- M1–M6 Rust 原型和自动化可靠性加固已经存在；M7 Phase B 的自动化与真实物理输入结果见本轮执行记录。
- M6 的菜单/释放墓碑、四类 repeat、回放顺序、UIPI 完整/零写入、高负载、布局/修饰/光标和两秒退出矩阵已完成；partial SendInput 与缺失硬件继续作为明确限制，详见 `check-fix-debug-list/M6-可靠性基线摘要.md`。
- M7 已确定为：纯 Rust + Win32 的 `inputflow-agent.exe` 常驻，C# + WinUI 3 的 `InputFlow.Settings.exe` 按需启动，以版本化 Windows Named Pipe 通信。
- M7 Phase A 已建立原生 WinUI 3 工程；当前固定为 unpackaged + framework-dependent。Phase E 已将它升级为正式设置程序，仍不安装 Hook、不直接写配置。
- M7 Phase B 已完成：ADR-005、完整具名键、logical/physical 双身份匹配、scan-code 回放和严格 Schema v2 均已落地；v1 字符串规则兼容读取并在内存中迁移。en-US 与 Microsoft Pinyin 下的 OEM 观察/回放，以及 Caps Lock 失败回放、命中消费、指示灯和 `F12` pending 恢复均有真实物理输入证据。
- M7 Phase C 已实现产品级 Rust runtime 与薄 agent：probe/agent 共用生命周期，规则可由 Hook owner 安全热替换，支持 capture session、结构化 apply/save 报告、Win32 托盘、单实例、Explorer 托盘恢复和 Release 无控制台。
- M7 Phase D 已实现 ADR-006、版本化 length-prefixed JSON 协议、当前用户/LocalSystem ACL 的 overlapped Named Pipe server、agent 全部控制面、有界事件推送，以及独立 C# `InputFlow.Protocol` client。
- M7 Phase E 已完成：ADR-007 / Schema v3、正式规则/录制/保存页面、应用级连接与 reconciliation、设置/诊断/关于及单实例均已落地；真实物理录制与三类代表规则、托盘/F12/UI 同步、UIA/高对比度/缩放、五分钟资源及 M6 Windows 矩阵均已验收。keypad Enter、独立播放键、中文 Narrator 语音环境和 partial SendInput 的限制单列保留。
- Phase F 的 ADR-008、方向状态机、Windows Hook 接入、严格 Schema v4／v1～v3 迁移、protocol capability、WinUI 编辑与有限预览已经实现；169 项 Rust 与 17 项 C# 自动测试通过。真实四方向、普通拖拽和约五分钟物理 move 仍待 F-PHY-01～07 验收，不能由合成测试替代。
- 项目不使用 Tauri、React、Node.js、npm、WebView2 或 Electron。

## 首版 Release 路线（2026-10-08）

用户已确定首个 release 必须包含鼠标方向；24／72 小时长时间运行测试和长期 daily-drive 放在首版发布后。当前 Phase F 代码和自动验证已完成，Windows 物理验收尚未执行，首版尚未发布。

| 顺序 | 工作 | 当前状态 |
|---|---|---|
| F／M8 | 键盘激活的鼠标四方向、配置、现有 WinUI 编辑／预览、短时高频实测 | 进行中；F0～F3 与自动回归完成，F4／F5 现场证据待执行 |
| G-PRE | 发布前自动回归、有限混合输入与关键异常恢复 | F 完成后执行 |
| H | x64 分发、依赖／路径、自启动、升级／移除及干净环境验证 | 待规划并实施 |
| RC／首个 Release | 固定最终包的有限检查、发布说明及校验和；建议 `v0.9.0-beta.1` Pre-release | 待完成，不要求长测 |
| G-POST | 24／72 小时运行、长期资源／恢复证据与后续补丁 | 首版发布后执行；不阻塞首版 |

Phase E 的 WinUI 3 设置程序已经完成，后续扩展现有程序。首版方向动作继续使用现有键盘组合；URL、启动程序、设备区分、序列／层和原生触控板手势另开后续任务。

下一步按 `docs/RELEASE_ROADMAP.md` 和 tag_6 Phase F 任务执行 F-PHY-01～07；真实物理证据收口后才转 `check-fix-debug-list/tag_6_InputFlow-首个Release收尾与发布任务.md`。旧 tag_5／tag_5.1 保留为历史背景，M6 不变量与已接受 ADR 继续有效。

## 产品原则

InputFlow 应像一把扳手，而不是常驻的大型桌面套件：

- agent 是唯一 Hook、托盘、规则运行时、正式配置和 IPC 服务所有者。
- 设置程序不安装 Hook、不直接写正式配置、不隐藏到托盘；关闭最后一个窗口后进程退出。
- agent 不加载 .NET、WinUI、WebView 或 JavaScript 运行时。
- Hook 热路径不执行 UI、磁盘、网络、无界分配或无界队列。
- F12（或已验证的替代组合）保留为紧急旁路。
- UI 只能提交草稿；agent 负责验证、原子保存和热应用，失败时保留旧规则。

## 已验证的开发环境

下表于 2026-09-29 在当前 Windows 工作区重新探测；完整组件和部署证据见 `docs/BUILD_WINDOWS.md`。

| 项目 | 已验证值 |
|---|---|
| Windows | 注册表产品名 Windows 10 Pro 25H2；实际内核 10.0.26200.9457，x64 |
| Rust | rustc 1.98.1；`stable-x86_64-pc-windows-msvc` |
| Windows SDK | 10.0.26100.0 |
| Visual Studio Build Tools | 2022 17.14.41；2026 18.10.2（未安装 WinUI workload） |
| .NET | SDK 10.0.401；MSBuild 18.9.11；Windows Desktop Runtime 10.0.12 |
| WinUI / Windows App SDK | CLI 模板包 0.0.7-alpha；Windows App SDK 2.5.1；BuildTools 10.0.28000.2705；Windows App Runtime 2.5.1 x64 |
| Cargo workspace | engine / config / runtime / windows / protocol / probe-cli / agent；测试数量以实际执行结果为准 |

## 构建

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p probe-cli
```

WinUI 3 设置程序：

```powershell
dotnet restore .\apps\settings-winui\InputFlow.Settings.slnx -p:Configuration=Debug
dotnet restore .\apps\settings-winui\InputFlow.Settings.slnx -p:Configuration=Release
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Debug --no-restore
dotnet build .\apps\settings-winui\InputFlow.Settings.slnx -c Release --no-restore
dotnet run --project .\apps\settings-winui\InputFlow.Protocol.ContractTests\InputFlow.Protocol.ContractTests.csproj -c Debug --no-build
dotnet run --project .\apps\settings-winui\InputFlow.Settings.Core.Tests\InputFlow.Settings.Core.Tests.csproj -c Debug --no-build
```

也可运行 `scripts/build-windows.ps1` 执行完整 Rust + C# 验证链。

Debug 可执行文件生成在 `apps/settings-winui/InputFlow.Settings/bin/Debug/net10.0-windows10.0.26100.0/win-x64/InputFlow.Settings.exe`。当前工程依赖 .NET Desktop Runtime 10 和 Windows App Runtime 2.5；没有这些运行库的目标机需先安装。完整命令、部署比较和未验证项见 `docs/BUILD_WINDOWS.md`。

## 仓库结构

```text
inputflow/
├── Cargo.toml
├── README.md
├── Steps.md
├── InputFlow-项目规划.md
├── apps/
│   ├── probe-cli/            # 共用 runtime 的诊断控制台
│   ├── inputflow-agent/      # M7 Phase C 常驻 Rust/Win32 agent
│   └── settings-winui/       # WinUI 设置、纯 .NET 状态核心、协议客户端及 contract runners
├── crates/
│   ├── inputflow-engine/
│   ├── inputflow-config/
│   ├── inputflow-runtime/    # agent/probe 共用生命周期与配置事务
│   ├── inputflow-windows/
│   └── inputflow-protocol/   # Phase D wire DTO、codec、有界 server 与测试
├── docs/
│   ├── PROJECT_PLAN.md
│   ├── BUILD_WINDOWS.md
│   ├── RELEASE_ROADMAP.md      # 首版范围、阶段顺序与发布后长测
│   └── decisions/
│       ├── ADR-000-仓库结构与技术选型.md
│       ├── ADR-001-组合匹配与回放协议.md
│       ├── ADR-002-时序规则与冲突策略.md
│       ├── ADR-003-可靠性配置暂停旁路诊断恢复与性能采样.md
│       ├── ADR-004-Rust常驻Agent与WinUI3设置程序.md
│       ├── ADR-005-逻辑键与物理键身份及Schema-v2.md
│       ├── ADR-006-版本化Named-Pipe协议与安全边界.md
│       ├── ADR-007-规则启停与Schema-v3.md
│       └── ADR-008-鼠标方向规则与直通策略.md
├── fixtures/
│   └── config/               # v1/v2/v3/v4 Rust/C# 跨语言 golden fixtures
└── check-fix-debug-list/
    ├── M6-可靠性基线摘要.md
    ├── tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md
    ├── tag_5.1_InputFlow-M7-Phase-E-WinUI3设置程序实施任务.md
    ├── tag_5_InputFlow-M7-WinUI3架构与输入扩展记录.md
    ├── tag_6_InputFlow-Phase-F-鼠标方向实施任务.md
    ├── tag_6_InputFlow-首个Release收尾与发布任务.md
    ├── tag_6_InputFlow-发布后Phase-G长时间运行任务.md
    ├── tag_6_InputFlow-Phase-F与首版发布执行记录.md
    └── archive/              # 已废弃的历史任务与修复记录
```

当前已完成 tag_5 Phase A–E；M6／Phase E 的证据见联合验收记录。Phase F/M8 的代码与自动验证已完成，真实 Windows 输入和资源观察待 tag_6 F4／F5 收口；首版必须完成 F，再执行 G-PRE／H／RC，发布后才进行 G-POST 长测。

## 配置键身份、启停与鼠标方向（Schema v4）

新保存统一使用显式键身份：

```json
{ "match": "logical", "key": "CapsLock" }
{ "match": "physical", "scan_code": 58, "extended": false }
```

logical 表达键的 Windows 语义，physical 表达 scan code 位置；同一事件同时命中时 exact physical 规则优先。旧 Schema v1 的 `"LeftCtrl"`、`"C"` 等字符串继续可读，并确定迁移为 logical，不根据当前布局猜测。完整样例见 `fixtures/config/`，设计和回放规则见 ADR-005。

Schema v3 在每条规则加入持久化 `enabled`。Schema v4 新增 `mouse_direction` trigger；v1／v2／v3 读取时在内存中确定迁移，禁用规则保留完整内容和顺序，但不进入运行时冲突检查/索引，`rule_count` 只统计启用规则。新保存统一写 v4，启用或保存前仍由 agent 对整份草稿重新验证。详见 ADR-007 与 ADR-008。

## 已知限制

- 完整键盘映射与 Schema v1～v4 迁移已实现自动测试；正式 UI 已接入有界、可取消的单输入 capture 和动态布局键名。Caps/OEM/方向/主 Enter 与 Fn 音量键已通过页面物理录制和读回；本机没有 keypad Enter 和独立播放键，故这两项未实测。
- Caps/Num/Scroll Lock 有 toggle 语义；自动测试已覆盖 Caps 暂存、失败、命中、repeat、pause/overflow 和精确 down/up 回放，Caps 的真实目标窗口、键盘指示灯及 `F12` pending 恢复也已通过。Num/Scroll Lock 的本机物理状态测试仍未执行。
- OEM 符号键的稳定 identity 是 `Oem*` 或显式 physical scan；UI 通过 Windows API 动态显示当前布局名称，但不会把显示文字写进配置。
- 鼠标方向规则已实现显式键盘激活、四方向固定组、净位移阈值、偏轴容差、超时、一次命中及 release tombstone；普通 move 始终直通。真实四方向、普通点击／拖拽、多屏和约五分钟高频物理 move 仍未执行，当前不能宣称 Phase F 已完成。
- `SendInput` 受 UIPI、焦点和当前修饰状态影响，不能承诺 100% 原样回放。
- 进程强杀时已暂扣的历史输入无法保证恢复。
- 设置程序关闭后完全退出、agent 持续运行已验证；Release 五分钟负载下 stats/callback 分位、工作集、线程、句柄及 Hook 存活已记录。该单机结果不是所有设备的性能保证。


- 首版最终分发包、干净机运行、自启动／升级／移除尚未验收；开发机 Release 构建不等于发布准备完成。
- 24／72 小时长时间运行验收尚未执行，已按用户决定安排在首版发布后；首版定位公开测试版本，不承诺已证明长期稳定。
