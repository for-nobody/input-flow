# InputFlow Phase F／M8 完成记录

> [!CAUTION]
> 本文是 2026-10-08 完成的 Phase F 历史证据。项目当前进度只看
> [`../../status/CURRENT_STATUS.md`](../../status/CURRENT_STATUS.md)。
>
> 文档类型：已完成阶段记录
>
> 状态：完成（含明确环境与性能限制）
>
> 最后更新：2026-10-08（Australia/Brisbane）

本文记录键盘激活鼠标四方向从设计、实现、缺陷修复到真实 Windows 验收的最终结果。G-PRE、
分发阶段 H、RC 和 G-POST 不属于本记录。

## 1. 基线与环境

| 字段 | 实际值 |
|---|---|
| Phase F 原实现提交 | `3c3a158821614419ec6b5b4b000f94c8d9f6cbdf` |
| 现场验收开始时文档基线 | `e542045`，分支 `main` |
| 最终验证输入 | 基于 `e542045` 的当前工作树；现场修复尚未在本文中虚构提交号 |
| Windows／架构 | Windows 10 Pro，DisplayVersion 26H2，build 26300.9457，x64，交互会话 1，非提升 |
| Rust／.NET | Rust/Cargo 1.98.1；.NET SDK 10.0.401；MSBuild 18.9.11 |
| Windows 工具链 | Windows SDK 10.0.26100.0；Windows App SDK 2.5.1；VS 2026 18.x x64 工具链 |
| 显示环境 | 单显示器；系统报告 1440×900、工作区 1440×852；注册表 AppliedDPI 120 |
| 输入环境 | 用户实际使用蓝牙无线鼠标；非提升会话无法可靠枚举设备型号／polling rate |
| 隔离配置 | `target/acceptance/phase-f-20261008-152228/config.json`，F8 四方向，80 px／500 ms／40 px |
| 目标与证据 | WinForms 消息目标 PID 12920；会话目录 `target/acceptance/phase-f-20261008-152228/` |

方向动作分别为 Left=`Ctrl+C`、Right=`Ctrl+V`、Up=`Ctrl+Z`、Down=`Ctrl+Y`。F12 保持紧急
暂停键。真实验收使用 Release Agent 和 Release Settings；脚本只做状态、资源和目标消息采集，
不冒充物理手势。

## 2. 最终设计与实现

- matcher 使用同一键的固定四槽方向组；净位移按主轴判断，等轴不命中，偏轴容差为包含边界；
  每次真实按住最多命中一次，只有真实 Up 后重新武装。
- activation Down／repeat 暂扣在既有有界 FIFO；失败、取消和超时按序回放；命中后保留 release
  tombstone。mouse move 永远直通，不进入 pending，不修改光标。
- Schema v4 严格表示方向、距离、时间窗和偏轴；v1／v2／v3 保持可读并确定迁移；wire v1 不变，
  handshake 能力为 `config_v4`。
- WinUI 提供四方向编辑、保存／读回和有限本地预览；预览不执行动作，也不保存轨迹。
- Agent PE 内嵌 `asInvoker`、`PerMonitorV2` manifest，并在 Hook owner 线程显式设置
  `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2`。方向起点优先使用同一 `MSLLHOOKSTRUCT` 来源的
  最近坐标，启动期才使用物理光标 API 作为 fallback。
- Release Settings 不再依赖禁用的反射 JSON 序列化；协议 frame、请求和配置文档使用显式
  `Utf8JsonWriter` 契约。
- hold-delay 遥测不再让匹配完成后的 F8 repeat 启动新窗口；这只修正统计，不改变 matcher 行为。

完整方向语义见
[`../../decisions/ADR-008-鼠标方向规则与直通策略.md`](../../decisions/ADR-008-鼠标方向规则与直通策略.md)。

## 3. 现场发现并修复的缺陷

### 3.1 Release Settings 启动失败

首次启动 Release Settings 报反射序列化已禁用。Agent 仍在线，故障在裁剪后的 C# 客户端握手前。
将 reflection-backed serialize 路径改为显式 writer 后，Release UI 能连接，live handshake、status、
get／validate／apply config、pause／resume、event、stats 和 capture／cancel 均通过。

### 3.2 缩放环境下方向错判

修复前用户和屏幕光标明确向左移动，matcher 却可能命中 Down。诊断证明 activation origin 来自被
DPI 虚拟化的 API 坐标，而低级鼠标 Hook endpoint 是 per-monitor-aware 坐标；例如 origin
`(734,494)` 与 endpoint `(743,1040)` 不能直接相减。单独换成 `GetPhysicalCursorPos` 仍未对齐。

最终采用嵌入 PerMonitorV2 manifest、Hook 线程显式 DPI awareness，并让 origin／endpoint 都来自
同一 Hook 坐标源。修复后左方向代表轨迹为 `(883,726) → (779,734)`，dx=-104、dy=+8；上、右、
下也分别得到与可见移动一致的同源坐标。蓝牙传输不是方向特定因素，已由可见光标和同源日志排除。

最终 `target/release/inputflow-agent.exe` 的 manifest 已用 Windows Manifest Tool 回读，包含
`dpiAware=true/pm` 与 `dpiAwareness=PerMonitorV2`。

### 3.3 hold-delay 假性长尾

一次上方向测试出现 94 秒 hold-delay，但匹配日志均在 500 ms 内。原因是成功命中后的 auto-repeat
错误开启新的遥测窗口，直到后续命令才关闭。修复后 repeat 不启动窗口；定向回归通过，最终方向样本
p99／max 为 75,404 µs，未再出现跨测试假值。

## 4. 最终自动与联合验证

| 命令／范围 | 最终结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `cargo test --workspace` | **171／171**：agent 4、config 32、engine 87、protocol 12、runtime 6、windows 30 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| `cargo build -p probe-cli` | 通过 |
| `cargo build -p inputflow-agent --release` | 通过 |
| WinUI solution Debug／Release | 均 0 warning、0 error |
| C# protocol contract | 6／6，通过 |
| C# Settings Core | 11／11，通过 |
| `scripts/build-windows.ps1 -SkipRestore` | 完整通过 |

受限终端中的第一次最终联合运行在 .NET 尝试访问用户级首次使用目录时被沙箱拒绝；Rust 171／171、
fmt、Clippy 和 Release Agent 在该轮已经通过。随后在正常本机会话以同一入口完整重跑，所有项目通过。
这属于执行环境权限，不是源码或测试失败。

## 5. F-PHY-01～07 结果

| 编号 | 结论 | 实际证据摘要 |
|---|---|---|
| F-PHY-01 | 通过 | Release UI 在线；四行 F8 方向规则、80／500／40、enabled、动作和顺序正确。禁用 Left 后保存，磁盘独立读回仅该规则禁用；重开后读回一致，再启用保存无错误。 |
| F-PHY-02 | 通过 | 修复后 Left 44、Up 12、Right 11、Down 14 次隔离命中；均为完整 Ctrl+C/Z/V/Y。repeat 不增加动作，真实 Up 后可再次触发；failed/dropped 为 0。 |
| F-PHY-03 | 通过 | 22 个边界候选产生 17 个有序 F8 回放和 5 个合法主轴动作；覆盖抖动、距离不足、提前释放、超时、偏轴／等轴和另一鼠标按钮取消。光标始终正常移动。 |
| F-PHY-04 | 通过 | 有限预览命中不执行动作，第二次 Esc 取消；两轮 F12、确定性 IPC pause 和 live config replace 均在候选期安全冲刷旧 F8，1/1 回放完整，无重复、粘键或迟到覆盖。 |
| F-PHY-05 | 通过，含性能例外 | 299.438 秒、297 个有效资源样本；真实持续 move、键盘、点击、菜单及远高于要求的 F8 负载后 Hook 仍工作，151 个输出全部成功，资源有界。两类延迟参考线例外已保留并调查。 |
| F-PHY-06 | 通过 | 用户在记事本、浏览器和资源管理器完成普通输入、菜单和拖拽；observed +592、callback +6,957，但 hold/output 均保持 206，无误触发或可见干扰。 |
| F-PHY-07 | 通过 | 三轮命中后保持 F8 → F12 pause → 暂停中释放 → resume，tombstone 均保留；最终命中后托盘正常退出，drain 内观察 F8 Up 后自行停止，无 `shutdown_limit` 或孤立 Up。 |

### 5.1 四方向与边界细节

- Left 的 44 个初始 F8 Down 恰好对应 44 个 `f8-mouse-left` 与 44 个完整 Ctrl+C；20 个 repeat
  没有额外动作。
- Up 新增 12 个 Ctrl+Z；Right 新增 11 个 Ctrl+V；Down 新增 14 个 Ctrl+Y，另有 4 个 repeat
  未重触发。四轮均无其他方向串扰。
- 边界运行的两组提前释放回放同时包含 tagged F8 Down／Up；15 个超时候选先回放 Down，再让物理
  repeat 和最终 Up 正常通过。500 ms 窗口的实测 hold-delay max 为 515,459 µs。
- 斜线仅在主轴足够占优且偏轴不超过 40 px 时命中；近等轴或偏轴过量保持候选并回放，不依赖规则
  JSON 顺序选择方向。

### 5.2 pause／replace／preview

- 干净 IPC pause：observed 3372→3373，约 45 ms 后 owner 处理 pause，`held_events=1`、1/1
  replay、`output_complete=true`；释放并 resume 后回到 ready。
- 干净 config replace：observed 3482→3483，约 49 ms 后 apply；runtime pause 1/1、四条规则替换、
  committed persistence 和 backup 均成功，无 recovery required。
- F-PHY-07 暂停分支的最后一轮中，seq 7490 开始 F8，Right 命中后 seq 7529 暂停，seq 7531
  在暂停中观察 F8 Up，seq 7532～7533 恢复。目标只看到完整 Ctrl+V 和 F12，不见 F8。

### 5.3 正常退出 drain

最终退出分支从 seq 7546 的真实 F8 Down 开始，Right 从 `(337,1041)` 移至 `(430,1041)` 后输出
完整 4/4 Ctrl+V。用户保持 F8，通过托盘选择 Exit，并在两秒内释放。Agent 记录：

```text
shutdown_drain: waiting up to 2000ms for consumed input releases
[seq=007679] kbd Up logical=F8 ... injected=false repeat=false
stopped: observed=7680 output_sent=210 output_failed=0 output_dropped=0 hook_panicked=false logger_panicked=false
```

本轮没有 `shutdown_limit`；PID 3384 自行消失，未强杀。目标只记录动作，不记录 F8 Down／Up，证明
Hook 卸载前 tombstone 已平衡消费。M6 历史记录中的 >2 秒上限和风险仍有效；本轮重新覆盖了当前方向
实现直接影响的 ≤2 秒干净分支。

## 6. 五分钟资源结果

原始采样为 `target/acceptance/phase-f-20261008-152228/fphy05-resource-samples.csv`：

- 297／297 查询成功，经过 299,438 ms；observed +2,916、callback +34,225、hold +151、
  output sent +151，failed/dropped 均 +0。
- 目标独立记录普通键盘、左右点击、上下文菜单、121 个方向动作 batch 起点和 28 个 F8 回放起点。
- Agent CPU +8.75 s；working set 13,049,856..13,754,368 B；private
  2,519,040..3,584,000 B；threads=9；handles=179..180。
- Settings CPU +4.15625 s；working set 97,456,128..110,407,680 B；private
  75,653,120..76,173,312 B；threads=20..26；handles=1,105..1,117。
- stats RTT p50=4.059 ms、p95=14.214 ms、p99=91.338 ms、max=136.574 ms；10／297 超过
  20 ms 参考线。没有请求错误、状态故障或资源无界增长。
- callback 最终 p99=182 µs，低于 5 ms 参考；累计 max 在 10.1 s 和 76.7 s 分别升至
  315,672／316,384 µs，超过 100 ms 参考。前者对应密集 F8 repeat＋click，后者对应 Ctrl+C
  action＋右键菜单切换；该口径包含下游 `CallNextHookEx`、recorder lock 和系统调度。

上述尖峰没有被隐藏或改写。两次之后 callback／物理输入继续增长，Agent 最终 ready，输出无失败／
丢弃，因此 F-PHY-05 的稳定条件通过；它们保留为 G-PRE／发布后长测需要继续观察的性能例外，
不提升参考线。

## 7. 仍然存在的限制

- 本机只有一个显示器；负坐标和 i32 边界有确定性回归，但真实跨屏、跨 DPI、热插拔未验证。
- 未取得鼠标型号或 polling rate；125／500／1000 Hz 仍只代表合成确定性序列，不冒充硬件频率。
- 第三方软件若移动光标但不带 injected 标记，首版无法可靠区分，ADR-008 的限制不变。
- keypad Enter、独立播放键、中文 Narrator 实际语音和 partial `SendInput` 继续沿用 Phase E 的明确
  硬件／环境限制。
- stats RTT 和 callback max 有上述已调查尖峰；五分钟短测不等于 24／72 小时稳定性证明。
- 最终分发包、干净环境、自启动、升级、移除、许可／签名和远端 release 尚未开始；它们属于
  G-PRE／H／RC。首版仍不得宣称已经完成发布后长期稳定性验证。

## 8. 结论

Phase F F0～F5、F-PHY-01～07、受影响的 release tombstone／pause／replace／shutdown 路径和最终
联合构建均已收口。现场发现的 Release JSON 与 DPI 方向缺陷已保留修复前证据，并由修复后的真实
四方向结果和自动回归验证。**Phase F 判定为完成（含第 7 节限制）**。

下一执行入口是 [`../../tasks/FIRST_RELEASE.md`](../../tasks/FIRST_RELEASE.md) 的 G-PRE；24／72 小时长测
仍按既定决定放在首个 release 之后。
