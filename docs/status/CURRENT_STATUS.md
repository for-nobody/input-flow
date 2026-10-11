# InputFlow 当前状态

> 文档类型：当前进度唯一权威
>
> 最后更新：2026-10-11（Australia/Brisbane）
>
> 代码基线：`main`；`v0.9.0` 已作为 GitHub Pre-release 发布，tag 指向 `c18a0a8...`

## 当前结论

InputFlow 已完成 M1～M7、Phase F／M8 和首版发布前 G-PRE。键盘激活鼠标四方向、Schema v4、
IPC、WinUI、真实边界、普通输入／拖拽、约 16 分钟混合输入，以及 UI／Pipe／配置／恢复、暂停和
退出回归均已收口；没有发现新的核心输入或配置阻断缺陷。

首版发布任务的 **H 已完成**。便携工程包、依赖、路径、自启动、升级／移除模拟和未来 Schema 降级
保护已完成；全新 Windows x64 环境也已通过前后端启动、四方向规则命中与重开读回，以及一次真实
重启登录自启动／禁用验收。`v0.9.0` 的 MIT License、版本面、许可附件、发布说明、干净构建、固定
zip、自动／IPC／Settings 壳层和精确包物理输入 smoke 全部通过。首个 release 已发布。

RC-01 英语／国际化实现和两次 Narrator 修补已固定并推送到 GitHub `origin/main`；最终候选构建提交为
`c18a0a80f004451f98bb80bfb7b90c66255845bb`。Settings 具备 `en-US`／`zh-CN` 资源、system／English／
简体中文选择和独立持久化。最终精确 ZIP 已通过完整自动门槛、双语运行、UIA 持久化、规则保存、物理
输入、Narrator、125%／150% 布局、Agent 100 轮和 live IPC。2026-10-11 14:46（Australia/Brisbane），
annotated tag `v0.9.0` 已推送并精确解析到该提交，GitHub Pre-release 已发布；两个附件从公开 Release
重新下载后哈希一致。L01／L03／L04 因中文或第三语言 Windows 环境不可得保持非阻断 `NOT TESTED`。
当前结论为 **v0.9.0 RELEASED；进入 G-POST**。

当前执行入口是 [`../tasks/POST_RELEASE_SOAK.md`](../tasks/POST_RELEASE_SOAK.md)。首版任务已归档至
[`../archive/first-release/FIRST_RELEASE.md`](../archive/first-release/FIRST_RELEASE.md)。Phase F 的完整历史
证据见 [`../archive/phase-f/PHASE_F.md`](../archive/phase-f/PHASE_F.md)；G-PRE 完整结果见
[`../archive/first-release/G_PRE.md`](../archive/first-release/G_PRE.md)。H 与 RC 记录见
[`../archive/first-release/FIRST_RELEASE_H_EXECUTION.md`](../archive/first-release/FIRST_RELEASE_H_EXECUTION.md)
和 [`../archive/first-release/FIRST_RELEASE_RC_EXECUTION.md`](../archive/first-release/FIRST_RELEASE_RC_EXECUTION.md)。
RC-01 的本地实施和复验矩阵见
[`../releases/patches/RC-01-WinUI3-i18n-en-US-verification.md`](../releases/patches/RC-01-WinUI3-i18n-en-US-verification.md)。

## 阶段状态

| 阶段 | 状态 | 完成或进入条件 |
|---|---|---|
| M1～M6 | 完成 | 原型、匹配器与可靠性加固已收口；证据已归档 |
| M7 Phase A～E | 完成 | Rust/Win32 Agent、Named Pipe、完整键盘、WinUI 设置和联合验收已收口 |
| Phase F／M8 | **完成** | F0～F5、F-PHY-01～07、受影响 M6 路径和最终联合构建通过 |
| G-PRE | **完成** | 自动门槛、15 分 53 秒资源采样、混合输入和 PRE-01～09 可用环境矩阵通过 |
| H | **完成** | H0～H5 与 DIST-01～07 按声明支持范围通过 |
| RC／首个 release | **完成；v0.9.0 已发布** | tag 精确指向 `c18a0a8...`；Pre-release 两个附件已上传并下载复验 |
| G-POST | **进入条件已满足；待开始** | 按已发布包执行 1～2 小时、24 小时、72 小时和 daily-drive |

## 最近已验证证据

远端发布结果：

- GitHub Pre-release：`https://github.com/for-nobody/input-flow/releases/tag/v0.9.0`；非 Draft，发布时间
  为 `2026-10-11T04:46:14Z`。
- annotated tag object 为 `f348f3a9f28ec4673d05de2ddc2611fa6cbce96c`，peeled commit 为
  `c18a0a80f004451f98bb80bfb7b90c66255845bb`。
- `InputFlow-0.9.0-win-x64.zip` 远端大小 94,155,158 bytes，GitHub digest 和重新下载实算 SHA-256
  均为 `D2C1BE6148E2F3833DEEA9C99D9C882F0BABD2A030A1CFD480D1364387D79D2C`。
- `SHA256SUMS.txt` 远端大小 95 bytes，下载文件 SHA-256 为
  `C79158362C43CF88994E9E5D7492BA32EA1B3B4AAEC9E0C96CCBD966ED63F2A9`；其中声明的 ZIP 哈希与实算一致。

RC 固定包结果：

- 最终构建提交 `bc48c36d73b94106d53fe192175dc005f756fe25`；`-RequireClean` 完整入口退出码 0，
  Rust **172／172**、C# protocol **7／7**、Settings Core **12／12**，WinUI Debug／Release 均 0 warning、
  0 error。
- `InputFlow-0.9.0-win-x64.zip` 为 94,128,454 bytes，SHA-256
  `A287489C5224DE45685B2B89D799C44472384CF22B12C98E13B3C318BC004537`；解压 245,226,071 bytes、
  579 个文件。manifest 提交匹配、`worktree_dirty=false`、MIT、Pre-release、私有构建路径已重映射。
- 精确 zip 的全新解压副本通过 Agent 100 轮 smoke、C# live IPC contract、真实 Settings 窗口启动／正常
  关闭与 Agent 干净停止。包内 PDB、用户配置／日志／marker、仓库／构建用户绝对路径命中均为 0。
- 用户在精确 zip 上确认四方向命中、距离不足／提前释放／右键取消、F12 暂停恢复、关闭 Settings 后
  Agent 继续运行和托盘正常退出均符合预期。日志最终为 `observed=299 output_sent=26`，failed/dropped
  均为 0，无 hook/logger panic 或 `shutdown_limit`；进程与 running marker 均已清除。

RC-01 本地补丁证据：

- 两套资源各 **293** 键且集合一致；85 个 `x:Uid`、动态资源键、UIA 附加属性和硬编码属性扫描通过。
- `scripts/build-windows.ps1 -SkipRestore` 通过：Rust **172／172**、C# protocol **7／7**、Settings Core
  **14／14**，WinUI Debug／Release 均 0 warning、0 error；四种 Debug／Release × en-US／zh-CN
  runtime smoke 均退出 0。
- 最终干净候选 `InputFlow-0.9.0-win-x64.zip` 为 94,155,158 bytes，SHA-256
  `D2C1BE6148E2F3833DEEA9C99D9C882F0BABD2A030A1CFD480D1364387D79D2C`；manifest 指向 `c18a0a8`，
  `worktree_dirty=false`、MIT、Pre-release、私有路径已重映射，解压 245,295,355 bytes、580 个条目、
  PRI 2,243,776 bytes、50 个第三方许可文件且禁止条目为 0。
- 最终精确 ZIP 通过 Settings en-US／zh-CN、English → 简体中文 → English → System UIA 链路、Agent
  100 轮与真实 Named Pipe live contract；停止记录 `output_failed=0`、`output_dropped=0`，无
  hook/logger panic，进程、running marker 和测试语言偏好均已清理。
- 用户在最终候选上确认 L08 草稿边界、L09 创建／禁用／保存／删除／恢复、L10 四方向与取消／F12、
  L11 双语键盘浏览、L12 中文错误和英语 Pause／Resume Narrator 通知均符合预期；最终规则恢复 4 条启用，
  Agent 物理输入记录为 `observed_events=362`、`output_batches_sent=25`、failed/dropped 均为 0。
- 125%（120 DPI）和 150%（144 DPI）的中英文设置页、规则编辑器、错误态和长文本无裁切、重叠、横向
  滚动或不可达控件；四张最终候选截图已保存到补丁证据目录。原显示缩放和语言偏好均已恢复。

G-PRE 固定提交的自动与联合门槛：

- Rust workspace：**171／171**；agent 4、config 32、engine 87、protocol 12、runtime 6、
  windows 30。`fmt`、严格 Clippy、probe 和 Release Agent 构建均通过。
- C# protocol contract：6／6；Settings Core：11／11。
- WinUI solution Debug／Release：均 0 warning、0 error。
- `scripts/build-windows.ps1`：含 restore 的完整链在获准网络访问后从头通过。
- Release Agent 的最终 PE manifest 已回读确认 `asInvoker` 和 `PerMonitorV2`。

真实 Windows 结果：

- 真实目标运行 17 分 39 秒；资源采样 941 个／953.320 秒。用户确认 Ctrl+C／V／Z／Y、普通输入、
  菜单和拖拽无明显丢键、粘键、漏动作或误触发。
- Agent 观察 6,067 个事件和 252 个方向候选；269 个输出全部完成，failed/dropped 均为 0。
- Settings 完整打开／关闭 20 次，Agent PID 不变且线程／句柄无逐次增长；10 次配置 apply 全部
  settled，Settings 异常结束／重开、Pipe 重连和 Agent 异常 marker 恢复通过。
- F8 候选中 F12、UI 与托盘 pause／resume 均保持释放归属；恢复后真实四方向继续工作。
- Win+L 锁屏／解锁后没有过期候选或异常触发。最终按住 F8 退出时，两秒 drain 内收到真实 Up，
  无孤立 Up、`shutdown_limit` 或线程 panic。

H 本机分发结果：

- 未来 Schema 修复后的完整门槛为 Rust **172／172**、C# protocol 6／6、Settings Core **12／12**，
  WinUI Debug／Release 均 0 warning、0 error；严格 Clippy、probe 与 Release Agent 构建通过。
- `InputFlow-0.1.0-win-x64.zip` 为双 self-contained、未裁剪的 x64 工程包，SHA-256 为
  `D20D04525E2161F66EBBD01EA2BC67E63841523D335C670CB8571AC6C32EFA1B`；目录约 243.4 MB、zip
  约 93.8 MB、526 个文件，PDB／用户配置／日志均未入包。该包生成时 manifest 如实记录
  `worktree_dirty=true`，因此是 H 工程包而不是 RC。
- 修复 WinUI publish 遗漏项目 PRI 导致的延迟 `0xc000027b`；最终包含 2,201,344-byte
  `InputFlow.Settings.pri`。最终包 Agent／Settings 从非程序中文工作目录正常启动和退出。
- Agent 实际需要 Microsoft Visual C++ Redistributable 2015–2022 x64 的 `VCRUNTIME140.dll`；.NET
  与 Windows App SDK 由包携带。该前置条件、未签名提示和 SHA-256 核对已写入用户指南。
- 包内自启动 enable／status／disable CMD 从非程序中文工作目录实际通过，目标和工作目录正确，
  最终恢复禁用。便携切换／移除模拟保留配置和禁用状态；v5 配置实际 apply 被明确拒绝且文件哈希
  不变。
- 2026-10-10 用户手动重启后，登录仅自动启动 1 个最终包 Agent，Settings 为 0；IPC `ready`、未暂停、
  无错误，日志无异常恢复或托盘创建失败。随后 Startup 已恢复禁用，快捷方式删除，当前 Agent 不被
  强制结束。
- 2026-10-10 用户在一套全新 Windows 环境确认当前工程包可以正常启动和运行，规则能够添加并识别，
  Agent 与 Settings 均未见报错；系统没有要求另装 .NET 或 Windows App Runtime，符合双
  self-contained 发布预期。
- 该干净环境报告 `ProductName=Windows 10 Pro`、`DisplayVersion=26H2`、build `26300.9457`、64-bit；
  产品名可能与数值 build 不一致，因此发布支持证据以 build 为准。系统已有
  `VCRUNTIME140.dll 14.40.33816.0`，证明声明支持路径可用，不声称验证了缺失运行库路径。
- 用户按现场清单确认 Left／Right／Up／Down 分别真实命中，关闭并从托盘重开 Settings 后规则与参数
  正确、Agent 在线；启用自启动时状态为 `enabled_current_path`。重启登录后只有 Agent 常驻、Settings
  未启动且规则仍可识别；禁用后状态为 `disabled`、快捷方式目标和工作目录均清空。

## 已知限制与后续观察

- 单显示器环境没有提供真实跨屏、跨 DPI 或热插拔证据；负坐标／极值只有确定性回归。
- 未取得鼠标型号和 polling rate；125／500／1000 Hz 只代表合成序列。
- G-PRE 样本的 stats RTT p99=151.274 ms、max=1064.207 ms，超过本机参考线；callback 累计
  p99=0.340 ms、max=17.265 ms，在参考线内。尖峰后 Hook 与完整输出继续增长；G-POST 继续观察，
  不隐藏样本也不调高参考线。
- keypad Enter、独立播放键、不同系统的 Narrator 语音可用性和 partial `SendInput` 保持既有硬件／环境限制。
- G-PRE 本轮未新增睡眠／唤醒和提升完整性目标的物理证据；现有确定性边界继续有效。
- H 工程包、本机与全新 Windows 的分发／自启动证据、升级／移除模拟均已完成。缺少 VC++ runtime
  的真实启动失败没有通过删除系统 DLL 强制制造；首版支持范围明确要求 Windows x64 和 VC++
  Redistributable，并在包名、manifest 与用户指南中给出可理解说明。
- MIT License 已确定；代码签名不在本版范围。RC-01 已从固定干净提交生成、验收并作为 GitHub
  Pre-release 发布。
- 中文 Windows 的 system 匹配 L01／L04 和第三语言 Windows 回退 L03 因环境不可得保持 `NOT TESTED`；
  不用强制资源覆盖冒充系统匹配。Agent 托盘语言未与 Settings 偏好同步。
- 24／72 小时长测尚未执行；首版仍定位公开测试版本，不宣称长期稳定。

## 下一步

保持已发布 tag 和附件不变，按 [`../tasks/POST_RELEASE_SOAK.md`](../tasks/POST_RELEASE_SOAK.md) 进入
G-POST。先准备可跨进程重启、睡眠和多 session 的低频采样工具，再使用真正发布的 ZIP 开始约 1～2
小时真实工作观察；24／72 小时结果不得提前标记完成。发布后发现的修复进入后续 `v0.9.x`，不改写
`v0.9.0` tag。
