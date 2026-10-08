# InputFlow 当前状态

> 文档类型：当前进度唯一权威
>
> 最后更新：2026-10-08（Australia/Brisbane）
>
> 代码基线：`main` 的 `e542045` 加当前已验证 Phase F 现场修复工作树；最终提交号尚未生成

## 当前结论

InputFlow 已完成 M1～M7 和 Phase F／M8。键盘激活鼠标四方向、Schema v4、IPC、WinUI 编辑／
预览、真实四方向、边界取消、普通输入／拖拽、五分钟资源观察以及 pause／replace／正常退出回归
均已收口。现场发现的 Release JSON 序列化和 DPI 坐标混用缺陷已经修复并由新证据验证。

项目现在进入首版发布任务的 **G-PRE**，尚未开始分发阶段 H、RC 或实际远端 release。24／72
小时长测继续安排在首个 release 之后，不阻塞发布前有限回归。

当前唯一执行入口是 [`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md)。Phase F 的完整历史
证据见 [`../archive/phase-f/PHASE_F.md`](../archive/phase-f/PHASE_F.md)。

## 阶段状态

| 阶段 | 状态 | 完成或进入条件 |
|---|---|---|
| M1～M6 | 完成 | 原型、匹配器与可靠性加固已收口；证据已归档 |
| M7 Phase A～E | 完成 | Rust/Win32 Agent、Named Pipe、完整键盘、WinUI 设置和联合验收已收口 |
| Phase F／M8 | **完成** | F0～F5、F-PHY-01～07、受影响 M6 路径和最终联合构建通过 |
| G-PRE | **当前阶段** | 执行短时混合输入、UI／Pipe／配置／恢复和有限异常回归 |
| H | 未开始 | 完成分发、依赖、路径、自启动、升级／移除和干净环境验证 |
| RC／首个 release | 未开始／未发布 | 固定最终包、完成 smoke、说明、许可和校验和后按授权发布 |
| G-POST | 计划于首版发布后 | 24／72 小时长测和 daily-drive，不阻塞首版 |

## 最近已验证证据

最终 Phase F 工作树的自动与联合门槛：

- Rust workspace：**171／171**；agent 4、config 32、engine 87、protocol 12、runtime 6、
  windows 30。`fmt`、严格 Clippy、probe 和 Release Agent 构建均通过。
- C# protocol contract：6／6；Settings Core：11／11。
- WinUI solution Debug／Release：均 0 warning、0 error。
- `scripts/build-windows.ps1 -SkipRestore`：在正常本机会话完整通过。
- Release Agent 的最终 PE manifest 已回读确认 `asInvoker` 和 `PerMonitorV2`。

真实 Windows 结果：

- F8 四方向分别稳定输出 Ctrl+C／V／Z／Y；一次按住只命中一次，真实 Up 后重新武装，输出
  failed/dropped 保持 0。
- 抖动、距离不足、提前释放、超时、偏轴／等轴、F12、IPC pause、配置替换和预览取消符合
  ADR-008 与 M6 的输入归属。
- 299.438 秒混合输入采样共 297 个有效 stats 样本；Hook 在持续 move、键盘、点击、菜单和高于
  要求的方向负载后继续工作，Agent／Settings 资源有界。
- 记事本、浏览器和资源管理器中的普通输入、菜单和拖拽没有误触发方向动作。
- 命中后暂停期间的 F8 Up 被 tombstone 正确消费；托盘正常退出在两秒 drain 内收到 F8 Up 后
  自行停止，无孤立 Up、`shutdown_limit` 或线程 panic。

## 已知限制与后续观察

- 单显示器环境没有提供真实跨屏、跨 DPI 或热插拔证据；负坐标／极值只有确定性回归。
- 未取得鼠标型号和 polling rate；125／500／1000 Hz 只代表合成序列。
- 五分钟样本的 stats RTT p99=91.338 ms、max=136.574 ms，callback 累计 max=316.384 ms，
  超过本机参考线；尖峰已定位到密集输入／菜单／系统调度窗口，Hook 和输出继续工作。G-PRE 与
  G-POST 继续观察，不隐藏样本也不调高参考线。
- keypad Enter、独立播放键、中文 Narrator 实际语音和 partial `SendInput` 保持既有硬件／环境限制。
- 最终分发包、干净环境、自启动、升级、移除、许可／签名和远端 release 尚未验证。
- 24／72 小时长测尚未执行；首版仍定位公开测试版本，不宣称长期稳定。

## 下一步

按 [`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md) 执行 G-PRE：先固定新的工作树基线，完成
自动门槛和约 10～15 分钟混合输入，再覆盖 UI 开关、配置应用、Pipe 重连、Agent 恢复、F12／
托盘和受影响退出边界。G-PRE 通过后再进入 H；不要提前创建 G-POST 记录或等待长测。
