# 触控板 Stage 3 · Touch Click 集成记录

2026-11-19（Australia/Brisbane），开始于 `f44d0e0`。此前在 ASUS Zenbook 14 UX3405MA（Windows x64 build `26200.9974`）验证了前台 HID 接触和目标消息。
[1B](TOUCHPAD_STAGE_1B_PROBE.md) 和 [2B](TOUCHPAD_STAGE_2_TOUCH_CLICK.md) 保留了描述符、原始样本和 F7 两次历史保护停用。
F7 专项复测实际送达 10 组双击和 21 组单击，证据只覆盖已接收的接触。

## 接入基线与验收方法

- Agent 的 Raw Input 注册、设备描述符和接触计时由独立的隐藏 Win32 窗口线程管理。现有低级 Hook 线程继续拥有输入旁路和输出顺序。
- 正式配置持久化实验总开关、Touch Click 启用值及阈值。运行状态由 Agent 经 Named Pipe 读回。键鼠规则保存继续走现有事务。
- 手势带启停代次和发起目标。结束或取消旧手势时作废其输出授权。点击输出提交前再次核对旁路、代次、当前前台和指针目标。
- 每次物理尝试用独立采样按键划界。记录期望动作、Raw Input 报告数、DOWN／UP、判定及取消、合成请求与实际插入、目标窗口消息。零报告仍计入分母，原因保留为未知。

本轮先补独立探针中同一启用会话的证据，再在外部应用验收 Agent 的真实输出。

## 独立探针连续 F7 会话

交互用户的 `TapsEnabled` 为 `1`，Windows 单指轻点保持开启。读回依据为 [Microsoft 的 Precision Touchpad 设置说明](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-tuning-guidelines)。
继续使用 `0.3.1` 和 400 ms、500 ms、3.0 mm。日志 `experiments/touchpad-probe/target/debug/touchpad-probe-14932-1795049440057.log`。
`Y` 原生基线确认指针命中，取得一组物理按放 HID 边沿及目标未标记左键。随后仅开启一次 `F7`。

| 操作 | 可归因报告／接触 | 合成输入 | 文字框实际收到 |
|---|---|---|---|
| 左侧轻触 `Q` | 初次 33 份，明确物理按钮。第二次 10 份，64 ms 短触 | 第二次插入 2/2 | 第一组为原生左键按放，第二组为带标记左键按放 |
| 右侧轻触 `W` | 10 份，62 ms 短触 | 1 次插入 2/2 | 一组带标记左键按放 |
| 快速双触 `T` | 9 份，1 组中的两次接触均完整，41／10 ms | 2 次均插入 2/2 | 普通左键按下、抬起、双击按下、抬起，均带标记 |
| 原地长触 `E` | 452 份，三次 605／1213／1416 ms 长触 | 3 次均插入 2/2 | 3 组带标记右键按放均在抬手前送达，0 左键 |
| 物理按住拖选 `Y` | 初次物理按压在接触 34 ms 时取消合成，随后指针出框 | 0 次调用 | 一组原生按放，189 个字符的选区。出框后正常退回观察 |

操作者确认中间漏按 Enter，后续采样键结束了前一组。`Y` 后续另有观察模式下的按放，未算作 F7 内拖选。会话读取 2,989 份前台报告，读取错误为 0。
本轮连续 F7 内 7 次合成均插入 2/2，合成动作未收到额外左键，未出现 `button_or_token_guard`。短触抬起到目标首条按下／双击为 18.825～26.982 ms；长触门槛到目标右键按下为 41.944～50.462 ms。
原生轻点仍开启，历史冲突仍保留。本次按样本切换界定动作，尚不能据此统计操作者确认的零报告漏点率。

## 3A · Agent 实机后台只读

测试 Agent 用 `--observe-touchpad --no-tray --run-for-ms 500000 --config target/stage3-observation/config.json` 启动。正式设置未运行，记事本获得焦点。Agent 注册 `RIDEV_INPUTSINK` 后发现 1 台可解码设备，按物理毫米量测。
操作者完成短触、长触、慢滑。状态读回共 2,896 份归属报告，均为 `RIM_INPUTSINK`，会话累计 14 次完整接触、8 个短触意图、1 个在 501 ms 产生的长触意图。日志明确记录 `movement` 取消。累计包含定位等额外操作，不能用来计算这三次的逐试次漏点率。

随后操作者在原地长触中多次按 F12，实测产生三次状态翻转，最后为暂停。两次候选分别在 338 ms 和 117 ms 记为 `paused`。恢复由 Named Pipe 执行，1.3 秒后长触意图仍为 1，旧候选未触发。此时后台报告累计 3,135 份，合成请求和实际插入均为 **0**。

## 原生轻点关闭后的独立验证

操作者同意临时关闭 Windows 的单指轻点。读回 `TapsEnabled=0`。独立探针日志为 `experiments/touchpad-probe/target/debug/touchpad-probe-29396-1795050687976.log`。
只读基线中，物理按压仍有一组原生左键按放；一次 80 ms 短触有 12 份报告和完整 DOWN／UP，目标没有收到原生左键。

同一连续 F7 启用期间，左右侧各一次短触各有一组带标记左键，快速双触有两组按放和一个双击，1,040 ms 长触在抬手前送达一组带标记右键。五次合成调用均插入 2/2。`Y` 的物理按压在 98 ms 取消合成，目标收到一组原生按放并选中 71 个字符。拖选正常结束后，另一次移动使指针出框，探针停止输出。五个试次未见合成以外的轻触点击或按钮保护停用。会话正常关闭，共读取 2,226 份报告，读取错误为 0。

## 3B／3C · 正式设置与目标消息

以隔离的 Schema v5 配置启动普通权限 Agent。WinUI 首次打开时规则页无触控板区。总开关的草稿显示待应用，保存读回后规则页显示关闭的 Touch Click 和 400／500／3.0 参数。启用并保存规则后，规则页显示就绪。Named Pipe 读回两个开关均开、1 台可用设备、`output_allowed=true`。关闭 Settings 后以只读探针为消息目标，探针没有开启 F6 或 F7。

目标日志 `experiments/touchpad-probe/target/debug/touchpad-probe-16908-1795051077355.log` 将正式 Agent 标记记为 `source=agent`。

| 操作者确认的试次 | 目标探针报告与判定 | 目标窗口实际收到 |
|---|---|---|
| 左侧轻触 1 次 | 9 份，完整 DOWN／UP，60 ms 短触 | 1 组 Agent 左键按放 |
| 右侧轻触 1 次 | 7 份，完整 DOWN／UP，45 ms 短触 | 1 组 Agent 左键按放 |
| 快速双触 1 组 | 17 份，完整接触 2 次，48／60 ms 短触 | Agent 普通左键按下、抬起、双击按下、抬起，选中 7 个字符 |
| 长触 1 次 | 193 份，完整接触 1 次，抬手时长 1,377 ms | 抬手前 1 组 Agent 右键按放，0 左键 |
| 物理拖选 1 次 | 184 份，明确物理按钮，完整接触 1 次 | 1 组原生左键按放，选中 70 个字符，0 合成 |

四个点击试次加一次物理拖选的漏点为 **0/5**，同一次双触计为一个试次。无意右击试次的误右键为 **0/4**，重复点击为 **0**。目标四个短触抬手到消息为 14.081～22.062 ms。Agent 状态累计包含试次以外的定位操作，当时为 20 个输入请求、20 个实际插入、0 输出失败；该累计值不代替上表目标消息。

关闭 Settings 后，操作者在记事本确认单击取消选词、双击选中 `touchpad`，长触在抬手前打开右键菜单，手感未发现异常。重新聚焦时也读到了 `touchpad` 的选区。测试配置正常重启后仍为两个开关开启和 400／500／3.0，Agent 无需重新保存即报告就绪。

重启后的正式输出试次先成功单击，随后操作者在长触中按一次 F12。记事本未打开右键菜单，Agent 返回 `paused`，该轮长触意图为 0。暂停前累计 `2/2` 个输入对应先前的单击。Named Pipe 恢复后静置 1.3 秒，长触意图和输入计数保持不变，旧候选未补发。

操作者随后确认新的长触重新打开右键菜单，并手动恢复 Windows「单指轻点以单击」。读回非零值 `0xffffffff`，界面和操作者均确认开启。Agent 返回 `native_tap_enabled` 和 `output_allowed=false`。Windows 设置全程由操作者切换。

在单独授权的隔离配置中，以 Named Pipe 保存总开关关闭，确认输出禁用且触控板退出订阅。再保存一条禁用的 `F8+F9` 键盘规则，重新打开实验并读回 `click_enabled=true` 及原阈值。原生轻点已恢复，实际输出继续停用。临时规则随后移除，隔离配置复原。Settings Core 测试覆盖相同的隐藏草稿往返。

## 实现与自动验证

`inputflow-touchpad` 由探针和 Agent 共用，负责 HID 描述符解码、Raw Input 数据边界、接触追踪、手势判定及部分插入的单次释放。Agent 的独立消息窗口处理设备注册与生命周期，Hook 线程在统一顺序中重新核对点击资格并发送。Schema v5 保存两个开关和阈值，v1～v4 继续内存迁移。命名管道将正式开关与设备、实际输出和停用原因分别读回。设置页和规则页均通过现有草稿保存。

```powershell
cargo fmt --all -- --check
cargo test --locked --offline --workspace
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
cargo test --locked --offline --manifest-path experiments/touchpad-probe/Cargo.toml
dotnet build apps/settings-winui/InputFlow.Settings.slnx -c Debug --no-restore
dotnet build apps/settings-winui/InputFlow.Settings.slnx -c Release --no-restore
dotnet run --project apps/settings-winui/InputFlow.Settings.Core.Tests -c Debug --no-build
dotnet run --project apps/settings-winui/InputFlow.Protocol.ContractTests -c Debug --no-build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-localization.ps1
```

Rust workspace 183 项、独立探针 4 项、Settings Core 15 项、Protocol 7 项均通过。Rust 格式检查与严格 Clippy 通过。WinUI Debug／Release 均为 0 警告、0 错误。双语资源键检查和四组 WinUI 运行冒烟均通过，含触控板开关自动化名称以及触控点击的关状态文字。Agent 会话日志位于 `%LOCALAPPDATA%\InputFlow\agent.log`。

## 仍待逐项实测

额外的慢滑折返、双指滚动、物理右键和外接鼠标试次没有留下可分组的正式输出目标消息。慢滑取消的后台只读证据与物理左键拖选的正式证据已记录。锁屏、睡眠、设备热插拔、跨完整性目标、真实输出故障、双语 Narrator 和不同宿主机仍待实测。历史偶发零报告与未标记左键保留在原记录，当前试次没有得到新的复现。
