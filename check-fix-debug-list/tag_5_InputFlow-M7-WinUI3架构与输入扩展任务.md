# InputFlow tag_5 原任务：A–E 已完成，Phase F 与收尾转入 tag_6

> 用途：保留 A–E 历史设计背景及不变量；**不是当前开工入口**。后续以 `docs/RELEASE_ROADMAP.md` 和 tag_6 任务书执行。
>
> 日期：2026-09-27
>
> 基线：当年本地代码git仓库，归档内 Git HEAD 为 `2ead0a9`（`update again with new develop path of this project`）。开始前必须核对你实际工作区的 HEAD 和未提交内容；不要覆盖用户改动。

## 后续任务重定向（2026-10-02）

M6／Phase E 联合验收已经完成，见 `M7-Phase-E与M6-Windows实机验收记录.md`。本文第 1～8 节属于历史任务说明，旧“前置缺口”不等于当前代码仍有这些缺口；不要重新实现已经完成的 runtime／IPC／WinUI。

当前用户决定：首个 release 必须包含鼠标方向；24／72 小时长测和长期 daily-drive 在首版发布后。顺序为 F → G-PRE → H → RC → 首个 Pre-release → G-POST。

1. 当前功能：`tag_6_InputFlow-Phase-F-鼠标方向实施任务.md`。
2. F 完成后的短时可靠性／分发／RC／发布准备：`tag_6_InputFlow-首个Release收尾与发布任务.md`。
3. 首版发布后：`tag_6_InputFlow-发布后Phase-G长时间运行任务.md`。
4. 新记录：`tag_6_InputFlow-Phase-F与首版发布执行记录.md`；原 tag_5 记录不改写为新的结果。

本节及新任务替代旧 Phase F／收尾的执行安排；下面产品架构、M6 不变量和已经接受的 ADR 仍适用。鼠标方向 ADR 为 **ADR-008**，ADR-007 已由规则启停／Schema v3 占用。

## 0. 产品决定（不得重新改回旧方案）

桌面架构已经由用户确定：

1. **常驻部分**：纯 Rust + Win32，产品名暂称 `inputflow-agent.exe`。
2. **设置部分**：C# + WinUI 3 + Windows App SDK，产品名暂称 `InputFlow.Settings.exe`。
3. **进程间通信**：Windows Named Pipe，版本化协议。
4. agent 是唯一 Hook、matcher、正式配置、托盘和运行诊断所有者。
5. 设置程序只按需启动，关闭最后一个窗口后必须结束进程；不隐藏到托盘，不随系统启动。
6. 不使用 Tauri 2、React、Node.js、npm、WebView 或 Electron 作为项目技术栈。
7. 不把 WinUI/.NET 引入常驻 agent，也不在设置程序中安装第二套 Hook。

上述决定见 `docs/decisions/ADR-004-Rust常驻Agent与WinUI3设置程序.md`。除非出现有代码/官方文档/实机证据证明方案不可行，否则不要反复询问或替换技术栈；如出现阻塞，先报告证据和最小调整方案。

## 1. 历史开工前必读和基线核对（A–E）

按顺序阅读：

1. `README.md`
2. `InputFlow-项目规划.md` / `docs/PROJECT_PLAN.md`
3. `Steps.md`
4. `docs/BUILD_WINDOWS.md`
5. `docs/decisions/ADR-000` 至 `ADR-004`
6. `check-fix-debug-list/M6-可靠性基线摘要.md`
7. engine/config/windows/probe-cli 当前代码和测试

然后报告：

- 当前 HEAD、分支、工作区状态和 `AGENTS.md` 指令。
- 实际 Windows/Rust/Visual Studio/.NET/SDK 环境。
- 当前自动测试、fmt、clippy、build 的真实结果。
- M6 人工验收矩阵哪些已有真实记录，哪些仍未执行。
- 下文列出的静态观察是否仍成立；若代码已变化，引用新位置，不套用旧行号。

历史记录称 M6 自动化为 92/92 通过，但这不是本轮的新执行结果。必须亲自运行：

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p probe-cli
```

没有可信物理键鼠或目标消息窗口时，自动工作可以继续，但对应验收必须写“未执行”，不能以单测或无输入 smoke 冒充。

## 2. 历史前置缺口（A–E 开工时；当前须以代码和验收记录核对）

请先验证，不要无条件照抄结论：

### 2.1 平台生命周期尚是原型级全局安装

- `inputflow-windows/platform/windows.rs` 使用 `OnceLock<Mutex<Matcher>>`、`OnceLock<Key>` 等进程全局对象。
- `install_matcher` / `install_emergency_key` 只有第一次设置生效。
- 当前没有正式的 runtime owner、规则热替换、capture session、agent restart 或多客户端状态接口。
- pause/resume 已经通过 Hook 消息线程串行化；不得为了 GUI 方便重新从任意线程直接改 matcher/BYPASS。

### 2.2 配置 API 仍不足以服务正式 UI

- `Config`/`LoadedConfig` 已有版本化、校验、故障恢复和安全替换。
- Schema v1 的 key 是字符串；`Key::from_name` 只接受有限枚举，`Unknown(vk)` 无法从正式配置进入规则。
- `save() -> Result<(), String>` 对成功提交后的清理 warning 没有结构化返回通道。
- UI 不应复制 Rust 校验逻辑，也不应直接覆盖 `config.json`。

### 2.3 键盘身份覆盖有限

当前明确建模的大致为：左右修饰键、A–Z、0–9、F1–F24、Space/Enter/Escape/Tab/Backspace。尚缺：

- Caps Lock、Num Lock、Scroll Lock。
- 方向键、Home/End、Page Up/Down、Insert/Delete。
- OEM 符号键（如分号、引号、方括号、反斜杠、逗号、句号、斜杠、加减号等，具体名称须以 Windows VK/布局语义为准）。
- 数字键盘与 keypad Enter/运算键。
- Print Screen、Pause、Apps/Menu。
- 常见音量、媒体、浏览器等键。

Hook 事件已有 `vkCode`、`scanCode` 和 extended 信息，但当前回放主要用 VK，配置没有表达“逻辑键”和“物理键位置”的差异。符号键不能只靠英文字符名称硬编码，因为键盘布局会改变显示和语义。

### 2.4 鼠标移动当前被忽略

- `WH_MOUSE_LL` 会收到移动事件，`MSLLHOOKSTRUCT.pt` 提供屏幕坐标。
- 当前 `mouse_proc` 只让鼠标按钮进入 matcher；move/wheel 直接向下传播。
- `MouseKind` 没有正式 Move 规则语义。
- 高频 move 可能达到数百或上千事件/秒；不能把每一点放入 pending、动态分配、逐点写日志或同步发送给 UI。

## 3. 实施纪律与阶段门槛

1. 每一阶段先写行为/线程/事件序列和失败模型，再写测试，再修改代码。
2. 先比较至少两种可行方案；选择时说明输入正确性、Hook 耗时、内存、跨线程顺序、崩溃和兼容成本。
3. 一次只完成一个可验收阶段，建议每阶段一个独立提交。不要一次生成庞大 UI 后再补协议和测试。
4. M6 人工矩阵未通过前，可以做 ADR、协议、Schema、纯引擎代码和静态 WinUI 骨架；不得把真实规则启用/保存接线描述为稳定完成。
5. 所有新 Win32 `unsafe` 继续集中在平台层，并写安全不变量。
6. 代码注释、标识符、测试名和 commit message 使用英文；文档、ADR、研究日志使用中文。
7. 不安装驱动、不修改系统级输入设置、不自动提升权限、不静默安装 Visual Studio/SDK。
8. 不删除 `probe-cli`；它继续用于诊断和对照。

## 4. Phase A：固定构建链与工程边界

### A1. 环境探测

按 `docs/BUILD_WINDOWS.md` 记录真实 Visual Studio、.NET、Windows SDK、Windows App SDK 和 WinUI 模板。不要直接沿用文档中的示例版本。

### A2. 最小 WinUI 3 smoke

在 `apps/settings-winui/` 用当前官方模板建立最小 C# WinUI 3 设置程序，只验证：

- 可以 restore/build/run。
- 打开一个原生窗口。
- 关闭最后窗口后进程退出。
- 没有 Hook、没有配置保存、没有虚假运行状态。

对 packaged/unpackaged、framework-dependent/self-contained 做简短比较。初始倾向可用 unpackaged + framework-dependent，但必须以实际构建/首次运行/部署证据决定并写入 ADR/BUILD_WINDOWS。

### A3. 不允许的做法

- 不创建 npm/package.json/Vite/Tauri 项目。
- 不把 WinUI 项目塞进 Cargo workspace。
- 不把 Rust Hook 复制成 C# P/Invoke 实现。
- 不为了看到 UI 而先伪造保存成功或 agent 在线。

### A4. 验收

- Rust workspace 仍通过原有检查。
- WinUI 最小工程可独立构建/启动/退出。
- README/BUILD_WINDOWS 记录真实命令、版本和未验证项。

## 5. Phase B：完整键盘身份与 Schema v2

先建立新的 ADR（建议 ADR-005），比较以下模型：

1. 只扩展大枚举并继续按 VK 匹配。
2. 只按 scan code + extended 匹配。
3. 保存逻辑 VK 与物理 scan code/extended，并让规则声明 match mode。

ADR 至少回答：

- 字母、数字、OEM 符号、左右修饰、keypad Enter、媒体键如何区分。
- UI 录制一个键后默认采用逻辑身份还是物理位置；是否在高级设置允许切换。
- 当前键盘布局下的显示名如何通过 Windows API取得；为什么显示名不写成稳定配置 identity。
- `SendInput` 何时使用 VK、`KEYEVENTF_SCANCODE`、extended；字符/文本输出明确不等同于物理按键输出。
- `Unknown`/厂商键如何保真或拒绝，不能静默映射成别的键。
- Schema v1 如何兼容读取/迁移/回滚。

### B1. 最低覆盖集

至少实现并测试：

- `CapsLock`、`NumLock`、`ScrollLock`。
- `Left/Right/Up/Down`、Home、End、PageUp、PageDown、Insert、Delete。
- Windows OEM punctuation keys。
- Numpad0–9、Add/Subtract/Multiply/Divide/Decimal、keypad Enter（如可可靠区分）。
- PrintScreen、Pause、Apps/Menu。
- VolumeUp/Down/Mute、MediaPlayPause/Next/Previous/Stop（以实际 Hook 可见性为准）。

### B2. 锁定键专门测试

Caps Lock 等键至少覆盖：

- 普通无规则 down/up。
- 作为候选后失败回放。
- 命中并消费。
- auto-repeat 或系统特殊行为。
- pending 时 pause/F12/overflow/quit。
- 回放后 toggle 状态只变化预期次数。
- 设置录制时不意外改变规则运行状态。

### B3. 配置与协议

- Schema v2 必须明确、可序列化、拒绝未知字段/非法组合。
- 保留 v1 fixture 和迁移测试；不得让已有 `LeftCtrl`、`C` 等规则失效。
- Rust 生成/维护一份协议或 JSON Schema 作为 C# DTO 的权威来源，避免手写两套含义逐渐漂移；如不采用生成，必须提供跨语言 golden fixtures。

### B4. 验收

- key mapping/round-trip/config/engine/replay 自动测试覆盖最低集合。
- 至少 US 和用户常用布局做实际符号键录制/显示/回放记录。
- Caps Lock 的目标窗口和系统指示状态有实机证据。

## 6. Phase C：产品级 Rust Agent Runtime

在 `apps/inputflow-agent/` 建立薄入口，但先把可复用生命周期从 `probe-cli` 和全局安装函数中抽离。目标不是机械复制 `main.rs`。

### C1. Runtime 能力

- Start/ready/status/pause/resume/apply-config/begin-capture/cancel-capture/shutdown。
- Hook 线程继续拥有所有改变 matcher 时序状态的控制操作。
- 替换规则前必须处理 pending 和 consumed release tombstone，说明事务顺序。
- `OnceLock` 可以继续承载进程唯一 callback bridge，但不得阻止同一进程内安全替换 matcher/config；若重构为 handle/runtime，说明 callback 生命周期和悬垂引用不变量。
- 结构化 `SaveReport`/`ApplyReport`，保留成功后的 cleanup warning、持久化结果、运行时替换结果和是否需要恢复。

### C2. 托盘与生命周期

用 Win32 `Shell_NotifyIcon` 实现：

- 打开设置。
- 暂停/恢复。
- 显示当前状态（菜单文字/图标或 tooltip，具体风格保持简单）。
- 退出。

要求：

- 托盘 pause/resume 复用既有 Hook 线程控制路径。
- Explorer 重启后能恢复托盘图标，或明确记录尚未实现。
- 单实例，第二次启动应激活/打开设置或给出明确行为。
- Release 不显示控制台；诊断日志仍可控且默认匿名化。
- 正常退出遵守两秒 tombstone drain 限制和现有诊断。

### C3. 验收

- probe-cli 与 agent 共用 runtime，不出现两份分叉 Hook 逻辑。
- UI 不运行时 agent 可独立启动、暂停、恢复、应用已有配置和退出。
- 空闲 CPU/内存/句柄/线程有基线；反复暂停/恢复/打开设置请求后无持续增长。

## 7. Phase D：版本化 Named Pipe 协议

先建立新的 ADR（建议 ADR-006），至少比较 JSON length-prefix、JSON lines 和其他 framing；说明为何所选方案能处理消息边界、大小上限和恶意/损坏输入。

### D1. 最低消息集

- `handshake`
- `get_status`
- `get_config`
- `validate_config`
- `apply_config`
- `pause`
- `resume`
- `get_stats`
- `begin_capture`
- `cancel_capture`
- `subscribe_events` 或等价有界推送

每个请求必须带协议版本和 request id；错误要有机器可读 code 与可展示 message。规定最大消息长度、超时、取消、断线、重复请求和 agent 正在关闭时的行为。

### D2. 安全与顺序

- Pipe ACL 只允许当前用户/必要系统主体，不能默认开放给任意本地用户。
- 不接受前端传来的任意路径、shell 命令或未限制的文件访问。
- IPC worker 只解析/校验/排队；不得让 Hook callback 等待 pipe。
- `apply_config` 必须有可审计事务。至少明确：validate、写 temp/commit、Hook 线程替换、失败回滚和返回报告的顺序。
- UI 断开不改变 agent 当前规则；capture session 在断线/超时后自动结束。

### D3. 测试

- Rust protocol codec/unit tests。
- Rust server integration tests：部分帧、超长、错误 JSON、版本不匹配、并发客户端、超时、断线、重连。
- C# client 使用共享 schema/golden fixtures 做兼容测试。
- pause/apply 与物理输入的线程顺序仍通过 M6 序列/故障注入测试。

## 8. Phase E：WinUI 3 设置程序

### E1. 信息架构

使用原生 Windows 工具风格，保持紧凑，不做大面积营销卡片或无意义仪表盘：

- 状态：运行/暂停、Hook 健康、启用规则数、最近错误、主暂停开关。
- 规则：trigger chips → action chips、启用开关、编辑、删除、新建。
- 诊断：输出失败、callback/hold 分位、配置恢复警告、导出诊断。
- 设置：紧急键、登录启动、诊断级别、配置位置/恢复。
- 关于：版本、支持边界、许可证。

优先评估 `NavigationView`、`CommandBar`、`ToggleSwitch`、`InfoBar`、`ContentDialog`、`NumberBox`。支持窗口缩放、主题、高对比度、键盘导航、焦点顺序、AutomationProperties/屏幕阅读器名称。

### E2. 规则编辑器

- “录制输入”为主，分类选择器为辅。
- 录制必须显式开始、显示剩余时间、允许 Esc/按钮取消，并在 agent 断线时立即结束。
- UI 显示友好键名和可选高级身份信息，但只发送稳定 KeySpec，不保存本地化 label 作为 identity。
- 冲突和延迟警告来自 agent 的权威验证结果。
- 保存按钮必须等待 `ApplyReport`；失败时区分验证、持久化、运行时应用和 IPC 错误。

### E3. 生命周期

- settings 单实例；再次启动激活现有窗口。
- 关闭最后窗口后进程退出。
- agent 未运行时显示离线和恢复指引；不得静默启动第二个 Hook。
- 设置程序崩溃/强杀后 agent 和输入行为不变。
- 不做后台每秒高频 stats 轮询；页面可见时节流查询或用推送。

### E4. 验收

- 无需编辑 JSON 可完成当前代表规则。
- 可录制/选择 Caps Lock、一个 OEM 符号、方向键、numpad 键和媒体键。
- 可暂停/恢复并看到 agent 的确认报告。
- 关闭设置后 `InputFlow.Settings.exe` 消失，agent 继续工作且不加载 WinUI/.NET/WebView。

## 9. Phase F（已转入 tag_6）

本节旧实施细则已由 `tag_6_InputFlow-Phase-F-鼠标方向实施任务.md` 替代；首版必须完成键盘激活的鼠标四方向、同键方向组、配置／UI 扩展和短时实机验收。

先写 ADR-008，明确坐标／单位／时间、激活键 Down／repeat／Up、冲突、Schema、注入及一次触发，再测试／实现。move 全程直通、无逐点分配、不进入 pending、不复位光标；锁外 SendInput、owner 串行化和释放归属继续满足 M6。

24／72 小时长测与长期自用不属于 Phase F 或首版发布前门槛，发布后由 G-POST 执行。鼠标按钮激活、设备来源、序列／层和原生触控板不扩大首版必需范围。

## 10. 不可破坏的 M6 可靠性语义

所有阶段必须回归：

- consumed release tombstone。
- repeat 进入有界 FIFO，失败/暂停时按序回放。
- SendInput 0/部分/完整的诊断和旁路。
- timer 安装 ready 门槛。
- pause/resume/apply/quit 在 Hook owner 线程的串行顺序。
- callback stats 锁内快照、锁外排序。
- committed backup 优先 temp、5/3 代保留与唯一有效副本保护。
- 默认匿名日志和显式 debug/capture。
- 正常退出最多等待两秒排空墓碑；强杀限制如实保留。

不得为了 IPC、托盘或 UI 便利恢复旧 worker/ack 设计、跨线程直接改 matcher、Hook 内阻塞发送、无界队列或默认逐键日志。

## 11. Windows 人工验收矩阵

在现有 tag_4 矩阵基础上新增：

| 类别 | 必测内容 |
|---|---|
| M6 回归 | 菜单/墓碑、repeat、pause 顺序、UIPI、stats 高负载、正常退出 ≤/>2s |
| Agent | 托盘、Explorer 重启、单实例、登录启动、无控制台、干净退出 |
| IPC | 当前用户 ACL、断线、重连、版本不匹配、超时、UI 崩溃 |
| WinUI | 关闭即退出、反复打开、DPI、主题、高对比度、键盘导航、屏幕阅读器名称 |
| 完整键盘 | Caps/Num/Scroll Lock、左右/扩展、OEM、numpad、媒体键、US + 常用布局 |
| 配置 | v1 加载、v2 往返、迁移失败、原子保存、恢复 warning、应用事务 |
| 鼠标方向 | 抖动、阈值、超时、对角、跨屏、一次触发、高频持续移动 |
| 资源 | agent 空闲/高负载 CPU、工作集、private bytes、线程、句柄；设置关闭后 UI 进程消失 |

记录环境、规则、完整手势、预期、观察、日志路径和通过/失败/未执行。不要只写“看起来正常”。

## 12. 后续文档和交付（已转入 tag_6）

F／G-PRE／H／RC 的实施中更新 README、Steps、两份项目规划、BUILD_WINDOWS、新 ADR／fixture／research-log，并写入 `tag_6_InputFlow-Phase-F与首版发布执行记录.md`。

阶段报告继续记录：基线／问题、候选方案及选择、修改文件、实际命令结果、物理观察、资源和未测限制。旧 tag_5／Phase E 验收记录保留原结果，不用它代替新代码或新包的测试。

最终包、依赖、自启动、升级／移除、干净环境、版本／校验和、发布草稿和实际发布授权按首个 Release 收尾任务执行。不要把开发机 build 成功写成已发布。

## 13. 后续完成门槛（按新路线）

- A–E 的完成状态以最新 tag_5 和 M6／Phase E 记录为准，不重新按历史任务开工。
- “鼠标方向完成”按 tag_6 F 的完成清单判定。
- “首版可发布”按 `docs/RELEASE_ROADMAP.md`、Steps 和 tag_6 首版收尾清单判定，**不包含 24／72 小时或长期 daily-drive**。
- “长期运行已验证”只有 G-POST 的真实版本／时长／覆盖证据具备后才能说明；首版 Pre-release 不提前使用这一承诺。

## 14. 官方资料起点

- WinUI 3：https://learn.microsoft.com/en-us/windows/apps/winui/winui3/
- Windows App SDK：https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/
- Windows App SDK deployment：https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/deploy-overview
- Named Pipes：https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes
- Shell_NotifyIcon：https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw
- LowLevelKeyboardProc：https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
- LowLevelMouseProc：https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc
- KBDLLHOOKSTRUCT：https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct
- MSLLHOOKSTRUCT：https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-msllhookstruct
- Keyboard input overview：https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input
- Raw Input overview：https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input
- SendInput：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput

以最新官方文档和真实实机结果为准。若文档与当前代码假设冲突，先记录冲突和实验，再更新 ADR；不要只为“完成任务”选择最省事但破坏输入可靠性的方案。
