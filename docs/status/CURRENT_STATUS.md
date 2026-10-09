# InputFlow 当前状态

> 文档类型：当前进度唯一权威
>
> 最后更新：2026-10-10（Australia/Brisbane）
>
> 代码基线：`main`；H 实现与证据随本状态文档一同版本化，工程包的精确构建基线见 H 执行记录

## 当前结论

InputFlow 已完成 M1～M7、Phase F／M8 和首版发布前 G-PRE。键盘激活鼠标四方向、Schema v4、
IPC、WinUI、真实边界、普通输入／拖拽、约 16 分钟混合输入，以及 UI／Pipe／配置／恢复、暂停和
退出回归均已收口；没有发现新的核心输入或配置阻断缺陷。

项目当前仍在首版发布任务的 **H**。便携工程包、依赖、路径、自启动脚本、一次真实重启登录、升级／
移除模拟和未来 Schema 降级保护的本机工作已经完成；用户提供的全新 Windows 环境也已完成基础启动、
前后端运行和规则添加／识别，未见报错。H5 仍需补齐该环境的准确 Windows build、四方向真实命中与
重开读回、自启动登录及缺依赖／错误架构等未报告项。RC 与实际远端 release 尚未开始。24／72 小时
长测继续安排在首个 release 之后。

当前唯一执行入口是 [`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md)。Phase F 的完整历史
证据见 [`../archive/phase-f/PHASE_F.md`](../archive/phase-f/PHASE_F.md)；G-PRE 完整结果见
[`../archive/first-release/G_PRE.md`](../archive/first-release/G_PRE.md)。当前 H 记录见
[`../records/FIRST_RELEASE_H_EXECUTION.md`](../records/FIRST_RELEASE_H_EXECUTION.md)。

## 阶段状态

| 阶段 | 状态 | 完成或进入条件 |
|---|---|---|
| M1～M6 | 完成 | 原型、匹配器与可靠性加固已收口；证据已归档 |
| M7 Phase A～E | 完成 | Rust/Win32 Agent、Named Pipe、完整键盘、WinUI 设置和联合验收已收口 |
| Phase F／M8 | **完成** | F0～F5、F-PHY-01～07、受影响 M6 路径和最终联合构建通过 |
| G-PRE | **完成** | 自动门槛、15 分 53 秒资源采样、混合输入和 PRE-01～09 可用环境矩阵通过 |
| H | **当前阶段；干净环境基础运行通过** | 补齐 H5 剩余 DIST 现场证据后才能完成 |
| RC／首个 release | 未开始／未发布 | 固定最终包、完成 smoke、说明、许可和校验和后按授权发布 |
| G-POST | 计划于首版发布后 | 24／72 小时长测和 daily-drive，不阻塞首版 |

## 最近已验证证据

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
  self-contained 发布预期。该结果尚未记录准确 Windows build，也不能证明机器缺少 VC++ runtime。

## 已知限制与后续观察

- 单显示器环境没有提供真实跨屏、跨 DPI 或热插拔证据；负坐标／极值只有确定性回归。
- 未取得鼠标型号和 polling rate；125／500／1000 Hz 只代表合成序列。
- G-PRE 样本的 stats RTT p99=151.274 ms、max=1064.207 ms，超过本机参考线；callback 累计
  p99=0.340 ms、max=17.265 ms，在参考线内。尖峰后 Hook 与完整输出继续增长；G-POST 继续观察，
  不隐藏样本也不调高参考线。
- keypad Enter、独立播放键、中文 Narrator 实际语音和 partial `SendInput` 保持既有硬件／环境限制。
- G-PRE 本轮未新增睡眠／唤醒和提升完整性目标的物理证据；现有确定性边界继续有效。
- H 工程包、本机自启动／真实登录／升级／移除已完成；全新 Windows 的基础运行和规则识别已通过，
  但 H5 的完整四方向读回、自启动登录、缺依赖／错误架构等现场证据尚未齐全。许可／签名和远端
  release 仍未完成。
- 24／72 小时长测尚未执行；首版仍定位公开测试版本，不宣称长期稳定。

## 下一步

按 [`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md) 收尾 H：在当前全新 Windows 环境记录准确
系统 build，并补齐剩余 DIST-01／02／04／07 现场证据。详细步骤与已完成结果见
[`../records/FIRST_RELEASE_H_EXECUTION.md`](../records/FIRST_RELEASE_H_EXECUTION.md)。该项完成前 H 保持
未通过，不进入 RC，也不提前执行发布后 G-POST。
