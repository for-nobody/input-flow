# InputFlow Phase F／M8：首版鼠标方向规则实施任务

> 日期：2026-10-08
> 状态：进行中；F0～F3 代码／自动验证完成，F4 真实 Windows 输入与 F5 现场收口待执行。主入口：`00_InputFlow-文档交接与执行入口.md`、`docs/RELEASE_ROADMAP.md`。
> 用户已确定：鼠标方向必须进入首个 release；24／72 小时长测在首版发布之后，不能阻塞本任务或发布前收尾。  
> 本任务替代 tag_5 的 Phase F 执行细则，不重做已完成的 Phase A–E。

## 0. 任务目标与边界

让用户通过现有 WinUI 3 设置程序创建、编辑、启停和保存：

```text
按住一个键盘激活键
  + 鼠标 Left / Right / Up / Down
  + 最小距离
  + 最大时间窗
  + 偏轴容差
  -> 现有 KeyChord 动作
```

第一版必须支持四个方向，以及同一激活键的四方向规则组。每个激活键按住周期最多执行一个方向动作；释放后重新武装。激活键支持已有 logical／physical 身份，保留 exact physical 优先规则。

鼠标移动始终传给 Windows，包括候选中和命中之后；不缓存或回放鼠标轨迹、不复位光标、不调用 `SetCursorPos` 来伪装抑制。鼠标按钮激活、无激活手势、轨迹形状识别、设备区分、Raw Input 架构替换和原生触控板手势不在首版必需范围内。

动作只使用既有 `Action::KeyChord`，不加入打开网页、运行程序或新动作 DSL。

## 1. 开工基线

先阅读本轮入口、发布路线、项目规划、Steps、BUILD_WINDOWS、ADR-000～007、M6 摘要和最新 Phase E 联合验收记录。核对实际 Git HEAD、分支、未提交内容及 `AGENTS.md`，不假设压缩包等于当前工作树，不覆盖用户修改。

按已有联合构建入口验证基线；记录当前真实测试数量。历史 Rust 151／151、protocol 6／6、Settings Core 11／11 只是对照，不写成自己执行的新结果。

### 1.1 当前代码导航（按内容定位，不依赖旧行号）

| 范围 | 起点 | 本轮重点 |
|---|---|---|
| 事件与规则 | `crates/inputflow-engine/src/event.rs`、`rules.rs` | MouseKind／坐标、Trigger、方向组预编译及冲突 |
| 状态机 | `crates/inputflow-engine/src/matcher.rs`、`state.rs`、`pending.rs` | 单 active prefix、repeat、失败回放、释放墓碑 |
| 配置 | `crates/inputflow-config/src/config.rs`、`fixtures/config/` | 严格 Schema、enabled、迁移、原子保存 |
| 平台 | `crates/inputflow-windows/src/platform/windows.rs` | mouse Hook、owner 串行化、同步 SendInput 重入、安全降级 |
| 生命周期 | `crates/inputflow-runtime/src/lib.rs` | timer、pause／resume、replace、capture、shutdown |
| IPC | `crates/inputflow-protocol/`、`apps/inputflow-agent/src/ipc.rs` | handshake、能力和版本化配置、事件有界性 |
| UI／DTO | `apps/settings-winui/InputFlow.Settings.Core/`、`InputFlow.Protocol/`、`InputFlow.Settings/` | 草稿、保存核对、编辑器、capture session／generation |
| 实机工具 | `scripts/acceptance/` | 目标消息、资源采样、有限诊断扩展 |

基线中 MouseMove 是否已经进入 matcher、是否包含坐标与事件时间必须直接查代码；不能因为 enum 或观察路径存在就假定方向功能已实现。

## 2. F0：先完成 ADR-008

创建 `docs/decisions/ADR-008-鼠标方向规则与直通策略.md`。不复用 ADR-007；它已用于 Schema v3 规则启停。

ADR 应包含候选方案、最终选择、事件序列和以下明确答案。可以依据证据自主完成常规设计选择，不为每个字段等待用户批准。

### 2.1 坐标、距离和时间

- 采用屏幕坐标净位移，或其他有明确理由的算法；默认优先屏幕坐标净位移，不以路径总长度冒充向某方向移动。
- 激活时获取明确起点；说明键盘 Down 时坐标来自何处。不能把第一次 move 当作起点后，无意遗漏激活后第一段移动。
- 固定坐标单位，UI、配置、算法一致。首版优先采用 Hook 坐标对应的屏幕像素，明确 DPI／鼠标速度／屏幕边缘的影响，不承诺真实毫米或原始设备位移。
- 明确 x 向右、y 向下；Up 需要负 y 净位移。用安全的有符号宽整数计算差值，支持负坐标和极端输入，不发生溢出。
- 定义 `min_distance`、`max_duration`、偏轴容差的单位、默认值、合法区间；区分字段建议与已验证结果。
- 写清阈值等于时、时间窗等于时、逆向／折返、斜线和偏轴超标时的行为；不靠浮点近似产生不确定边界。
- 使用单调时间源；不要用可能因系统时间调整倒退的墙钟判定超时。睡眠后旧候选不能继续命中。

### 2.2 激活键 down／repeat／up 的归属

对每条路径列出 down／repeat／up 是直通、暂扣、回放还是消费。首选复用现有有界暂扣、失败回放和成功释放墓碑模型；如直通激活键更可靠，必须先说明它对普通输入、现有修饰键和输出动作的影响，再明确 UI 文案和测试。

必须覆盖：没有移动、距离不足、超时、错误方向、提前释放、其他键／按钮／滚轮到来、F12／pause、配置替换、capture、队列满、输出失败、正常退出和异常终止。

特别注意：既有待定 FIFO 只有 16 项。长时间按住会产生自动 repeat；方向状态不能无限等待，不能把第 17 项悄悄丢掉或为了方向功能绕过旧队列满降级。要测真实 repeat 和阈值组合，解释首版默认时间窗能否正常完成手势。move 绝不能进入该 FIFO。

候选超时／取消／命中后，仍按住激活键时不能因为 repeat 或后续 move 重新武装；必须等真实匹配的物理 Up。失败后不能凭空消费本该传给目标的 Up，成功后不能泄漏已消费 Down 的 Up。

### 2.3 多方向和既有规则冲突

- 同一激活键的四方向组合法；同一键同一方向的重复规则拒绝。
- 首版优先要求同组使用一致的距离／时间／容差，避免高频路径逐条搜索；若允许不同值，证明运行成本有固定上界且选择确定。
- 与 Hold、KeyChord、KeyMouseButton、HoldMouseButton 共享同一前缀时，默认沿用 ADR-002 的跨类型拒绝策略；不要静默改变优先级。
- physical／logical 对同一事件都可匹配时，沿用既有确定顺序。紧急旁路键不能被方向组抢占。
- 斜线可能同时满足两个方向时必须有确定策略，例如主轴胜出且相等不命中；不要依赖 JSON 顺序或 HashMap 遍历顺序。
- 初版继续单候选语义；不同时启动多个激活键状态而没有正式设计。

### 2.4 move 来源与光标跳变

- 沿用当前“注入事件不触发规则”的策略；明确本程序和其他软件 injected move 都不驱动方向命中。
- 注入 Up 不能清除真实物理 held／tombstone。程序产生的移动不能递归触发。
- 对可识别注入跳变，定义取消候选或重置基准，避免下一次物理 move 因旧起点误触发。
- 对无法可靠区分的系统／第三方光标跳变，明确限制，不宣称 Hook 能识别所有来源。
- 多屏负坐标不代表自动完成跨 DPI／热拔插验收；无硬件时保留明确未测范围。

### 2.5 Schema 与协议

默认设计为新 Schema v4，继续读取 v1／v2／v3，旧规则确定迁移且保持 enabled、身份、顺序和动作；读取不自动覆盖正式配置。最终编号以 ADR 决定为准，不在尚未实现前把 README 写成 v4 已完成。

方向配置需要稳定 trigger tag、字段名、严格 enum／区间校验、unknown-field 和缺字段行为。禁用规则仍需结构合法；只有启用规则参与索引和冲突。

Rust 与 C# 同步 fixture、DTO、validate／apply、get_config 读回；handshake 必须让不支持新结构的旧 UI／Agent 明确拒绝，不能悄悄删除方向规则。若保留 wire v1，说明 envelope／method 没变化且用 schema／capability 门槛控制；确有 wire 变化才升级协议。旧程序不能用不了解新配置的备份覆盖新版本唯一有效副本。

## 3. F1：纯算法和状态机

先写确定性事件序列测试，再实现平台无关识别器和必要 matcher 扩展。推荐单独的小模块；不把坐标数学散落进 UI 或 Hook。

| 测试组 | 必须覆盖 |
|---|---|
| 四方向 | 每个方向的阈值前、等于阈值、超过阈值 |
| 抖动与折返 | 小抖动；反向后正向；来回走很长但净位移不足 |
| 偏轴 | 容差前／等于／超过；对角线；主轴相等；不存在对应方向 |
| 时间 | 超时前／等于／之后；无 move 也取消；timer 与释放交错 |
| 按住周期 | 命中一次后继续移动／repeat 不重触发；释放后下一次可触发 |
| 输入归属 | 提前 Up、repeat、队列满、其他输入取消；无孤立 Up／重复回放 |
| 控制边界 | F12、pause／resume、disable、replace、capture、quit；旧候选不带入新配置 |
| 身份与来源 | logical／physical 优先、布局变化后释放、注入 move／Up、自身注入 |
| 坐标 | 负坐标、跨原点、大坐标、同一点重复、坐标跳变策略 |
| 配置 | 四方向组、重复／跨类型冲突、非法值、禁用规则、迁移及跨语言往返 |
| 压力模型 | 125／500／1000 Hz 时间序列；验证一次命中、固定空间和无 move pending 增长 |

合成 1000 Hz 是确定性模型，不是物理鼠标 polling rate 证据。测试应验证行为及资源边界，避免只照抄实现表达式。

## 4. F2：Hook／runtime 接入

1. move 热路径只更新固定大小状态；无逐点 heap allocation、无逐点日志、无磁盘／网络／GUI／阻塞发送。
2. 预编译方向组；每个 move 最多检查四个固定方向，不按全部配置条数线性扫描。
3. inactive／paused／bypass／无规则时快速直通。候选中、命中后和异常路径的 move 仍直通。
4. 决策和状态更新在既有 owner 边界内完成；输出复用现有同步执行和完整／零／部分插入报告。
5. **调用 SendInput 前释放 matcher 锁。** 事件和 timer 两条路径都需保留 Phase E 已修复的物理输入重入防死锁测试；不能新增锁内输出。
6. 首次命中先设置“一次性／已命中”状态，再允许可重入输出，避免重入导致重复动作。
7. 输出失败沿用安全旁路、stranded 输入诊断及墓碑，不只在 UI 弹一个错误。
8. pause／resume／apply／quit 仍经过 Hook owner 串行化；替换先处理旧候选和 pending，保留已消费释放归属。
9. 如果要预览，采用固定大小摘要和节流；背压丢预览而不阻塞 Hook。禁止向 Named Pipe 或 UI 同步发送每一点。
10. 平台 unsafe、timer、消息和句柄清理留在 Windows 层，写出新的清理不变量。

## 5. F3：WinUI 编辑和有限预览

复用现有规则列表、草稿、KeyPicker、RuleEnablementBinding、capture tracker、应用级 coordinator 和保存 reconciliation。

- 新增“鼠标方向”类型及激活键、方向、距离、时间和容差控件；动作继续用已有键盘组合编辑器。
- 相同键的四方向规则可逐条建立并保存；非法组参数／冲突由 Agent 权威校验，UI 显示可理解错误。
- 文案明确：按住激活键后移动、光标仍会移动、每次按住最多一次、单位及失败／取消行为。
- 普通 move 不能覆盖 KeyPicker 的单键录制结果。方向预览必须显式开启、可取消且有时间上限。
- 预览只展示方向、距离／进度和状态，不保存完整坐标轨迹；UI 关闭、Esc、session 过期或断线立即取消预览。
- 迟到结果依然受 session／generation 过滤。优先复用既有录制独占规则；预览时不执行真实动作，F12 始终可用。
- UI 不能直接写正式配置、复制 matcher 算法成为权威或安装第二套 Hook。
- 保存后关闭／重开读回；列表启用刷新不得再次清零其他规则。焦点、UIA 名称、DPI、窄窗口布局继续可用。

若预览需要扩展 protocol，应只增加有界摘要及明确 capability，写入 contract 和 fixture；不要新增通用远程输入流。

## 6. F4：短时 Windows 实机验收

完成自动化后，使用隔离配置和真实目标窗口，记录环境、实际包／HEAD、完整手势、预期／实际、日志路径与结论。

| 编号 | 操作 | 期望 |
|---|---|---|
| F-PHY-01 | UI 建立同一激活键的四方向组，保存并重开 | 字段、enabled、动作和规则顺序读回一致 |
| F-PHY-02 | 四方向分别越阈值；同次按住继续移动 | 每次只输出一个完整动作；释放后可再次触发 |
| F-PHY-03 | 抖动、距离不足、提前释放、超时、斜线 | 与 ADR 一致，激活键归属正确，光标正常移动 |
| F-PHY-04 | 候选中 F12／pause、修改配置、进入／取消预览 | 旧候选取消；无重复动作、粘键或迟到 UI 覆盖 |
| F-PHY-05 | 持续 move 约五分钟，同时少量打字／点击及 stats 查询 | Hook 继续观察输入，资源及延迟有记录，无无界增长 |
| F-PHY-06 | 记事本、浏览器、资源管理器普通输入和拖拽 | 未激活不命中，普通菜单／拖拽正常 |
| F-PHY-07 | 保持激活键进入暂停／正常退出 | down／up 与 drain 行为符合 M6，限制仍如实记录 |

资源报告至少包含 observed／callback 样本增量、p50／p95／p99／max、stats RTT、CPU 时间差及口径、working set、private bytes、线程、句柄、failed／dropped 增量。沿用本机短测参考线 stats p99 <20 ms、callback p99 <5 ms、max <100 ms；实际超线先调查并注明原因，不改数字或隐去尖峰。

不声称获得 Hook 安装存活的可靠系统通知；用真实物理输入的观察计数、目标消息和规则命中证明测试时段仍工作。无物理输入时“进程在运行／IPC ready”不足以证明 Hook 仍安装。

没有 1000 Hz 设备／第二屏时：确定性模型照常做；真实硬件按实际能力测，记录未验证范围，不为缺失硬件无限拖延首版。自动脚本注入不得充当物理手势。

## 7. F5：回归、文档和交付

- 执行 `scripts/build-windows.ps1` 和对本轮改动必要的定向检查，保留 fmt／Clippy／Rust／C# contract／Core／WinUI Debug／Release 门槛。
- 回归既有四类触发器、M6 墓碑／repeat／队列满、owner 顺序、UIPI 报告、timer ready、配置事务与两秒退出。未改动且版本一致的现场证据可引用；平台／输出／matcher 改动涉及的路径要重新验证。
- 更新 README、Steps、两份项目规划、BUILD_WINDOWS、ADR、fixture 说明、research-log 和 tag_6 执行记录；已完成与限制分开写。
- 不虚构测试数量、代码提交、设备 polling rate 或实机结果。失败记录要保留原始序列及修复后的新证据。
- 只在适用许可范围提交本地变更；不把代码实现完成写成已发布。

## 8. Phase F 完成门槛

- [x] ADR-008 决策完整；激活键归属、repeat、冲突、坐标、时间及 Schema 均确定。
- [x] 四方向与一次触发算法和状态机测试通过。
- [x] 版本化配置、迁移、Rust／C# fixtures／handshake／保存读回自动 contract 通过。
- [x] WinUI 已实现创建／编辑／启停／保存／有限预览；自动构建与 Core contract 通过，真实 UI 保存／重开待 F-PHY-01。
- [ ] 真实四方向手势及短时 move 负载有证据；硬件限制单列。
- [ ] M6 与 Phase E 自动关键路径无回退、文档已同步；受影响现场路径待 F4 后收口。

全部满足后将首版范围标记为 feature-complete，进入 `tag_6_InputFlow-首个Release收尾与发布任务.md` 的 G-PRE。**这里没有 24／72 小时、长期 daily-drive 或安装器完成条件。**

## 9. 官方资料起点

- LowLevelMouseProc：https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc
- MSLLHOOKSTRUCT：https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-msllhookstruct
- SendInput：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- GetCursorPos：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getcursorpos

实现时检查当时的官方 API 约束；资料是设计依据，真实正确性由事件测试和目标机观察共同验证。
