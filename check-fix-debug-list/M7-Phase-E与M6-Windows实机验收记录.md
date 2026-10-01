# InputFlow M7 Phase E 与 M6 Windows 实机验收记录

> 时间：2026-10-01～2026-10-02（Australia/Brisbane）
> 状态：已完成；Phase E 与 M6 实机门槛通过，硬件/环境不可用项和不可控 partial SendInput 单列保留
> 证据纪律：自动测试、无输入 lifecycle、脚本注入和真实物理输入分开记录；未观察项不写成通过

## 1. 环境

- Windows build `26200.9457` / x64，交互 session 1，单屏 `1440x900`。
- 用户桌面缩放基线为 200%（192 DPI）；验收期间实际切换 125%/150% 后恢复 200%。High Contrast 初始未开启；Narrator 初始未运行。
- 已配置 `en-US` 和 `zh-Hans-CN` / Microsoft Pinyin；后续布局验收在两者间切换。
- 当前自动化进程为普通权限；提升权限目标需用户确认 UAC。

## 2. 验收前修复与自动基线

- 确认 Phase E capture 存在真实启动竞态：`begin_capture` 返回 session id 前 UI 可以被第二个录制覆盖，取消也会直接返回。已增加 `CaptureUiIntentTracker`，启动时立即互斥，并把“启动中取消”传递到随后到达的 session id。
- 定向 core 测试覆盖“字段 A 开始 → 字段 B 再次点击被拒绝 → 启动中取消 → A 的 session id 到达 → 只取消 A”；Settings Core 为 10/10。
- 完整 `scripts/build-windows.ps1 -SkipRestore` 通过：Rust 148/148，fmt/Clippy/build 通过，WinUI Debug/Release 0 warning / 0 error，protocol 6/6，Settings Core 10/10。

## 3. 实机工具预检

- `scripts/acceptance/message-target.ps1` 实测获得非零窗口句柄，写入 `START` 和 `SHOWN`，预检进程已定向关闭。
- `scripts/acceptance/configs/*.json` 共5 份配置均由真实 `probe-cli` Hook/runtime 加载并干净退出；empty 为 0 条，四种触发器配置各 1 条，全部 exit code 0。这是配置/lifecycle 证据，不是物理触发证据。
- `sample-agent-resources.ps1` 已连接真实 `InputFlow.Agent.v1.1` pipe：3 秒预检连续 6 次成功，RTT 2.564–4.594 ms，无协议错误，agent 自动退出。

## 4. Phase E 短资源生命周期

证据：`target/acceptance/20261001-011929-resource-lifecycle/`。每段 10 秒、500 ms 采样，每段 20 行，所有 CSV `error` 为空。

| 阶段 | stats RTT p99/max | agent working set 首→尾 | agent private 首→尾 | 关键进程边界 |
| --- | ---: | ---: | ---: | --- |
| 仅 agent | 36.866/36.866 ms | 8,769,536 → 8,785,920 B | 1,511,424 → 1,523,712 B | 9 threads，101 handles；单个调度尖峰超过 20 ms，长稳态再判定 |
| 设置打开 | 6.861/6.861 ms | 8,843,264 → 8,843,264 B | 1,622,016 → 1,622,016 B | Settings working set 144,789,504 → 143,511,552 B，private 46,886,912 → 45,629,440 B |
| 设置关闭后 | 9.168/9.168 ms | 8,810,496 → 8,798,208 B | 1,531,904 → 1,499,136 B | Settings exit code 0；agent 继续并回到 8–9 threads/101–102 handles |

该结果验证冷启/开窗/关窗的短边界；不替代 5 分钟物理高负载长稳态。

## 5. 现场执行矩阵

| 编号 | 场景 | 当前状态 | 最终证据 |
| --- | --- | --- | --- |
| E-PHY-01 | Caps/OEM/方向/主 Enter/keypad Enter/媒体键 capture，保存后重启读回 | 本机可用键通过；keypad/播放键硬件不可用 | UI 观察 + config 前后快照 |
| E-FLOW-01 | hold、key+mouse、hold+mouse 的 UI 创建/保存/物理触发 | 通过；hold+mouse 同时保留一次队列滢出证据 | target messages + agent stats + UI 观察 |
| E-SYNC-01 | 托盘/F12/UI Active/Paused 双向同步 | 通过 | 用户可见状态 + agent log |
| E-A11Y-01 | 纯键盘、Narrator、UIA、高对比度、125/150/200% | 键盘、UIA、高对比度和缩放通过；中文 Narrator 语音受本机语言环境限制 | 用户观察 + UIA 树 + 系统 DPI/高对比度状态 |
| E-RES-01 | 5 分钟打字/点击/鼠标移动 + stats 长稳态 | Release 5 分钟通过 | resources CSV + target/agent log + 用户观察 |
| M6-01 | 菜单/左右键释放墓碑与 F12/pause 交错 | 通过；Left/Right + F12，Right + 外部 pause | `WM_CONTEXTMENU`/button down-up 序列 |
| M6-02 | 四类触发器的命中/失败/repeat/暂停/队列满 | 通过；Hold、KeyMouseButton、HoldMouseButton、KeyChord 四类矩阵齐全 | 隔离配置 + target messages |
| M6-03 | 失败回放后立即新物理输入的顺序 | 通过；KeyMouseButton 前缀 A、失败 G、新输入 H | 单调消息序列 |
| M6-04 | 普通/提升权限目标的完整/零/部分 SendInput | 普通完整插入、提升目标 replay/action 零写入通过；partial 未实机触发 | UAC 目标消息 + tagged extra info + Agent 输出报告 |
| M6-05 | 高负载输入与 stats 查询 | 通过；Release 下 297 次查询、5,458 个 callback 样本 | 与 E-RES-01 同一长稳态 CSV |
| M6-06 | 左右修饰、扩展键、布局、鼠标位置、既有物理修饰 | 通过；中英布局、左右修饰、方向键、两处光标与左 Shift 保持 | HKL/cursor/message 日志 |
| M6-07 | 按住已消费输入时的 <2 s / >2 s 退出 | 通过；F8 tombstone 的 drain 内释放与 2 s limit 均命中 | Agent shutdown 时序 + 目标消息 |
| C-TRAY-01 | Explorer 真实重启后托盘恢复 | 通过；图标与 Active 菜单自动恢复 | Explorer PID 前后 + 用户观察 + Agent 状态 |

## 6. 判定线

- 5 分钟内无 Hook 丢失、粘键/鼠标卡住、output failed/dropped；资源无持续单调增长。
- 长稳态 `get_stats` p99 < 20 ms，callback p99 < 5 ms、max < 100 ms。它们是本轮验收线，不是 Win32 API 最坏时间保证。
- 用户没有的实体键、无法稳定制造的 partial SendInput 不伪造“通过”；保留为明确的硬件/系统边界。

## 7. Phase E 物理 capture（第一批）

会话证据：`target/acceptance/20261001-013826-physical/capture-batch-1.log`。用户在正式 WinUI 页面逐项点击“录制”并使用笔记本内置键盘；所有页面显示均符合预期，Esc 取消没有覆盖字段。agent `--debug-input` 的非注入事件与 UI 结果交叉核对如下：

| 物理操作 | logical | scan | extended | 结果 |
| --- | --- | ---: | --- | --- |
| Esc 取消 | Escape | `0x01` | false | 通过；UI 显示已取消，字段未改写 |
| Caps Lock | CapsLock | `0x3A` | false | 通过 |
| `;` 物理 OEM 键 | Oem1 | `0x27` | false | 通过 |
| 左 / 上 / 右 / 下 | Left / Up / Right / Down | `0x4B` / `0x48` / `0x4D` / `0x50` | true | 通过；扩展键位正确 |
| 主键盘 Enter | Enter | `0x1C` | false | 通过 |
| Fn 未锁定时的功能区按键 | F1 / F2 / F3 | `0x3B` / `0x3C` / `0x3D` | false | 试验者随后发现 Fn Lock 未启用；这三次是普通 F 键，不作音量键结论 |
| Fn Lock 后静音 / 音量减 / 音量加 | VolumeMute / VolumeDown / VolumeUp | `0x20` / `0x2E` / `0x30` | true | 通过；正式 UI 显示 `D — VolumeMute`、`C — VolumeDown`、`B — VolumeUp`，与 Hook 日志一致 |

本机没有数字小键盘/keypad Enter，也没有独立播放类媒体键；两者记为“硬件不可用，未实测”，不用选择器或脚本冒充物证据。Fn Lock 后的三个标准音量键已实测通过，证据在 `capture-volume-fn-lock.log`。

用户将最后录制的 `VolumeUp` 写入禁用规则 `capture-probe`，页面“验证并保存”成功。关闭设置后原 PID 4556 已退出，agent PID 3928 持续；托盘 Open Settings 创建新 PID 3992，页面正确读回 `VolumeUp`。落盘快照 `config-after-capture.json` 与 UI 一致，实际 schema v3 为 `enabled:false` + `key_chord(VolumeUp,B)` + `LeftCtrl+C`。

## 8. 多规则启用竞态与长按语义修复

用户经正式 UI 保存 `ui-hold-f8` 后新建第二条规则，观察到列表刷新会把两条启用状态都反写为 false；继续在列表逐条启用也可复现。落盘故障快照 `config-after-toggle-bug.json` 确认三条新规则均被错误禁用，不是视觉假象。

根因是列表重建时 `ToggleSwitch.Toggled` 把控件初始化/复用事件当成用户操作。修复为每行独立的 `RuleEnablementBinding` + two-way binding：构造/同值初始化不回写，只有真实变值才修改对应 rule id。新增回归测试后 Settings Core 11/11，Debug WinUI build 0 warning / 0 error。修复后用户实测连续启用三条规则，既有状态不再被刷新清零。

`hold_mouse_button` 编辑页同时调整为“键盘键长按阈值”在前、“随后按下的鼠标按钮”在后，并明确鼠标按钮本身不需要长按。用户复验语义和布局清楚。

## 9. 首轮三类规则物理观察

配置读回确认 `ui-hold-f8`、`ui-key-mouse`、`ui-hold-mouse` 均为 `enabled:true`；第三条的键盘阈值实际落盘为 499 ms。证据为 `m6-repeat-overflow-agent.log` 和 `m6-repeat-overflow-target.log`：

- 02:23:12，F8 Hold 命中，目标只观察到一批注入 Ctrl+C，通过。
- 02:23:27，A + Right 首次命中，目标只观察到一批注入 Ctrl+C，没有随后 `WM_CONTEXTMENU`，释放墓碑通过。
- 02:23:41 起，F9 长按产生自动 repeat；在右键 Down 到达前，第 17 个待定事件使 16 项有界队列溢出，agent 按设计回放已保留前缀并进入旁路。因此稍后的 Right Down/Up 透传并出现 `WM_CONTEXTMENU`；后续 A+Right 也因 agent 尚在旁路而透传。这不是“成功命中后孤立 Right Up”，而是 M6 队列满降级路径的真实实机证据。

旧日志路径在已旁路后仍对每个事件重复记录 `queue_overflow: entering bypass`，会在高负载下放大日志。已改为仅在 platform bypass 由 false 转 true 时记录一次；这不改变队列滢出、回放或旁路语义。

将 `ui-hold-mouse` 的键盘阈值调为 200 ms 并恢复 agent 后，用户再次执行 A+Right 与 F9 达阈值后 Right，两次均符合预期。`hold-mouse-success-target.log` 显示两组完整 Ctrl Down / C Down / C Up / Ctrl Up，中间和之后均无 `WM_CONTEXTMENU`；`hold-mouse-success-agent.log` 确认物理 A/Right 与 F9 repeat/Right/Up 顺序。三类 Phase E 规则的 UI 创建、保存、读回和物理命中至此通过。

## 10. F12 / UI / 托盘双向同步

用户依次执行 F12 暂停、暂停时 A+Right、UI 恢复、活动时 A+Right、托盘 Pause、F12 恢复。页面与托盘的 Active/Paused 文字及 Pause/Resume 命令全部按预期双向更新。

`f12-ui-tray-sync-agent.log` 记录 F12 `seq=004886` 后 `suspended ON`，托盘命令记录 `tray: paused held=0 output_complete=true`，最后 F12 `seq=004946` 后 `suspended OFF`。`f12-ui-tray-sync-target.log` 同时证明：暂停时 A Down/char 与 Right Down/Up/`WM_CONTEXTMENU` 完整透传；UI 恢复后同一手势只产生注入 Ctrl+C，不出现菜单。F12 自身的 Down/Up 在目标可见，符合紧急键不被拦截的设计。

## 11. Right 释放墓碑与 F12 暂停交错

用户在目标窗口中按住 A，按住 Right，释放 A，再按 F12 进入暂停，最后释放 Right。`right-tombstone-f12-agent.log` 记录了 A/Right 触发 `ui-key-mouse`、完整输出 4/4，随后 F12 切换为 `suspended ON`，最后物理 Right Up 到达 Hook。

`right-tombstone-f12-target.log` 只有完整的 Ctrl Down / C Down / C Up / Ctrl Up 和 F12 Down/Up，没有孤立 Right Up，也没有 `WM_CONTEXTMENU`。因此 Right 释放墓碑在 F12 暂停边界通过。目标窗口的单条事件区只显示最新消息，Ctrl+C 不会以字面文字出现；本结论依据的是完整消息日志。

外部 pause 交错使用同一手势，但不按 F12：用户保持 Right，验收脚本通过 Named Pipe 发送 `pause`，页面显示暂停后用户再释放 Right。`right-tombstone-external-pause-response.json` 记录 17:40:17 暂停成功，`held_events=0`、`output_complete=true`。Agent 在 17:40:04 命中并完整输出 4/4，17:40:22 收到物理 Right Up；目标日志在 17:40:04 的 Ctrl+C 后没有新消息，因此没有孤立 Right Up 或 `WM_CONTEXTMENU`。Right 释放墓碑的外部 pause 边界通过。

首次 Left + F12 尝试未进入墓碑路径：A 在 Left Down 前累积至第 17 个待定事件，matcher 先回放 16 个事件并进入旁路，所以 Left 透传，紧随的 F12 执行恢复而非暂停。这不用作 Left 墓碑结论，但 `key-mouse-left-overflow-agent.log` 与 `key-mouse-left-overflow-target.log` 构成 KeyMouseButton 的真实 repeat/FIFO 队列满、有序回放和转旁路证据。

重试时 A 与 Left Down 紧邻，`left-tombstone-f12-agent.log` 记录 `ui-key-mouse` 命中并完整输出 4/4，然后 A Up、F12 `suspended ON`、最后物理 Left Up。`left-tombstone-f12-target.log` 只有完整 Ctrl+C 与 F12 Down/Up，没有 `WM_LBUTTONDOWN`/`WM_LBUTTONUP`；因此没有孤立 Left Up 泄漏。Left/Right 双按钮与 F12 交错，以及 Right 与外部 pause 交错均通过，M6-01 完成。

## 12. 失败回放与紧接新物理输入

在 `ui-key-mouse = A + Left` 的 Active 配置下，用户按住 A，立即按 G 使前缀失败，再紧接按 H。`key-mouse-failure-new-input-agent.log` 记录 A Down、G Down、`replay 2 held event(s)` 与 `inserted=2 requested=2 status=Complete`，随后才观测到 H Down。`key-mouse-failure-new-input-target.log` 中的 Down 顺序严格为 A、G、H，之后的 G Up、A Up、H Up 也与物理释放顺序一致。回放与新 Hook 输入之间的 owner-thread 屏障成立，M6-03 通过。

## 13. Hold 阈值前释放

用户在 Active 状态快速轻按 F8，按住时间低于 `ui-hold-f8` 的 500 ms 阈值。`hold-f8-early-release-agent.log` 记录 F8 Down/Up，然后回放 2/2 且输出完整；`hold-f8-early-release-target.log` 只有一组 F8 Down/Up，没有 Ctrl+C。Hold 的阈值前失败和 FIFO 回放通过。

将阈值临时调为 5000 ms 后，用户按住 F8 约 2 秒并在到达阈值前释放。`hold-f8-overflow-agent.log` 记录初次 Down + 16 个 repeat 待定事件，第 17 项使队列满，然后 `replay 16 held event(s)` 且 `inserted=16 requested=16 status=Complete`。`hold-f8-overflow-target.log` 只有 F8 Down/repeat/Up，没有 Ctrl+C。Hold 的 repeat/FIFO 队列满、有序回放和转旁路通过。

恢复 Active 后仍保持 5000 ms 阈值，用户按住 F8 并在半秒内按 F12。`hold-f8-pending-f12-agent.log` 记录 owner 线程先回放唯一的 F8 Down，输出 1/1 完整，再进入 `suspended ON`，最后观测到物理 F8 Up。`hold-f8-pending-f12-target.log` 按序为 F8 Down、F12 Down/Up、F8 Up，没有 Ctrl+C。Hold 的 pending pause/F12 冲刷和平衡释放通过，该触发器的命中、失败、repeat/队列满、暂停场景已齐全。

## 14. HoldMouseButton 阈值前鼠标失败

在 `ui-hold-mouse = F9(200 ms) + Right` 下，用户按下 F9 后立即右击，Right Down 在长按阈值前到达。`hold-mouse-early-button-agent.log` 记录 F9 Down、Right Down 与 `replay 2 held event(s)`，没有 matched/output action；`hold-mouse-early-button-target.log` 显示 F9 Down/Up 与随后的 `WM_CONTEXTMENU`，没有 Ctrl+C。用户同时确认可见右键菜单。这是完整的失败回放表现，HoldMouseButton 阈值前失败通过。

将阈值临时调为 5000 ms 并运行修复版 Agent 后，用户按住 F9，半秒内按 F12，确认暂停后释放 F9。`hold-mouse-pending-f12-agent.log` 记录 owner 线程回放唯一 F9 Down，输出 1/1 完整，再进入 `suspended ON`，最后观测到 F9 Up。`hold-mouse-pending-f12-target.log` 顺序为 F9 Down、F12 Down/Up、F9 Up，没有 Ctrl+C。HoldMouseButton 的命中、失败、pending pause、repeat/队列满场景至此齐全。

## 15. 回放期间物理释放重入死锁与修复

紧接上述 F9 + Right 失败回放后，用户将 `ui-hold-mouse` 阈值改为 5000 ms 并点击“验证并保存”，UI 报 `hook thread did not start control request within 2000ms; request cancelled`。现场核对证明请求尚未开始且已安全取消：磁盘和 runtime 仍为 200 ms，`apply_reconciliation=settled`，`state_revision=56`，无半提交。但 Hook 已不再观测输入，无法处理 owner-thread 控制消息。

根因由日志精确定位：Right Down 的失败路径在持有 matcher 的非重入互斥锁时同步调用 `SendInput`。用户几乎同时释放 F9，物理 F9 Up 在外层 `SendInput` 返回前重入同一 Hook 线程；它写出 `seq=006335` 后再次获取 matcher 锁，与外层回调自死锁。因此日志有 `replay 2 held event(s)` 和 F9 Up，却没有外层 `output:` 完成行。

修复后，事件决策先在 matcher 锁内完成，随后明确释放锁再做同步输出；若输出失败，再重新取锁标记 paused/bypass 和 stranded 事件。`WM_TIMER` 的 Hold 动作路径同样先取出 commands 并释放锁再输出。新增两条回归分别断言事件回放和 timer 动作输出期间 `matcher.try_lock()` 可成功；后续 UIPI 预检又增加一条回归，`inputflow-windows` 现为 27/27 通过，fmt 通过。

旧 Agent PID 3928 因死锁无法正常退出，核验路径后定向强制结束。修复版 Release 重建后以相同隔离配置和 `--debug-input` 启动为 PID 748；日志中 `previous_abnormal=true` 是此次受控强制结束的预期恢复证据。新进程 Hook/IPC ready，3 条启用规则、Active 状态与无最近错误均正常；同场保存与物理重入回归待继续。

修复版上同场保存 5000 ms 成功，磁盘与 runtime 均 settled。用户随后多次重复快速 F9 Down → Right click → F9 Up；`reentrant-replay-fixed-agent.log` 的主序列记录 F9/Right Down、`replay 2 held event(s)`、`output: inserted=2 requested=2 status=Complete`、物理 Right Up 和 F9 Up。之后 `reentrant-replay-fixed-status.json` 立即取得 ready/Active、settled、`last_error=null`，证明 Hook owner 和控制队列未再锁死。现场这次外层输出比物理 Up 更早完成，没有再命中旧日志的“Up 先于 output”微小时窗；该精确重入条件由新增的 matcher 锁可用性回归确定性覆盖。

## 16. KeyMouseButton pending 暂停

在恢复的 `ui-key-mouse = A + Right` 配置下，用户按住 A 而不按鼠标键，半秒内按 F12，确认暂停后释放 A。`key-mouse-pending-f12-agent.log` 记录 owner 线程回放 A Down 1/1 后进入 `suspended ON`，最后收到 A Up。`key-mouse-pending-f12-target.log` 顺序为 A Down/char、F12 Down/Up、A Up，没有 Ctrl+C。结合既有的命中、失败和 repeat/队列满证据，KeyMouseButton 矩阵齐全。

## 17. KeyChord 矩阵

用户通过正式 UI 新建并启用 `m6-key-chord = D + E -> LeftCtrl + C`，保存后 Agent 为 4 条规则、settled/Active。物理 D+E 连续两次命中；`key-chord-hit-agent.log` 每次均记录 matched 和 `inserted=4 requested=4 status=Complete`，D/E Up 随后被释放墓碑消费。`key-chord-hit-target.log` 只有完整 Ctrl Down / C Down / C Up / Ctrl Up，没有 D/E 泄漏。KeyChord 命中通过。

错误第二键使用 D → G。`key-chord-failure-agent.log` 记录 D/G Down、`replay 2 held event(s)` 和输出 2/2 完整；`key-chord-failure-target.log` 严格为 D Down/char、G Down/char、G Up、D Up，没有 Ctrl+C。KeyChord 失败 FIFO 回放通过。

pending 暂停使用 D Down → F12 → D Up。`key-chord-pending-f12-agent.log` 记录 matcher 先回放唯一的 D Down，输出 1/1 完整，再进入 `suspended ON`；`key-chord-pending-f12-target.log` 严格为 D Down/char、F12 Down/Up、D Up，没有 Ctrl+C。随后 `get_status` 返回 ready、settled、`suspended=true`、`last_error=null`。KeyChord 的 pending pause/F12 冲刷和平衡释放通过。

repeat/FIFO 队列满测试中，用户只长按 D 约 2 秒而不按 E。`key-chord-overflow-agent.log` 记录初次 D Down 与 16 个 repeat 待定事件；第 17 项触发 `queue_overflow: entering bypass`，随后 `replay 16 held event(s)` 且输出 16/16 完整。进入旁路后的其余 repeat 与最终 D Up 均透传。`key-chord-overflow-target.log` 只有连续 D Down/char 与一个最终 D Up，没有 Ctrl/C 消息。自本次 resume 起精确统计 `queue_overflow` 日志为 1 条，验证了日志放大修复；API 状态为 ready、settled、`suspended=true`、`last_error=null`。KeyChord 的 repeat/队列满、有序回放和转旁路通过，M6-02 四类触发器矩阵全部完成。

矩阵结束后通过正式 UI 删除临时 `m6-key-chord` 并验证保存。隔离配置读回为 4 条落盘规则：禁用的 `capture-probe` 与 3 条启用的正式验收规则；运行时只计启用项，返回 `rule_count=3`、ready/Active、settled、`last_error=null`。基线已恢复。

## 18. SendInput 完整/零写入与 UIPI 误报修复

验收使用增强版消息目标记录 `pid`、是否提升以及 `GetMessageExtraInfo`。普通目标 PID 9484 为非提升进程；提升目标 PID 10380 经 UAC 启动并记录 `elevated=True`。InputFlow 为注入事件设置的标记为 `0x494E5055`。

普通目标中的动作输出完整：`normal-target-extra-messages.log` 两次记录严格的 Ctrl Down / C Down / C Up / Ctrl Up，四个键盘消息均带 `extra=0x494E5055`；Agent 同时报告完整写入。因此普通权限完整插入通过。

首次对提升目标执行失败回放时发现了真实缺陷：Medium Agent 向 High 目标调用 `SendInput` 返回请求数量，Agent 因而报告 Complete，但提升目标只收到物理 Shift，完全没有带 InputFlow 标记的事件。该机器上的 API 返回值不足以证明跨完整性级别事件真正到达目标，旧实现会虚称历史输入已恢复。

Windows 输出层现增加 UIPI 前置判定：读取当前进程与前台目标进程的 token integrity RID 和 `TokenUIAccess`；当发送方没有 UIAccess 且前台目标完整性级别更高时，不调用 `SendInput`，而是确定性返回 `inserted=0`、`ERROR_ACCESS_DENIED (5)` 和 `ZeroInserted`，沿既有失败路径进入旁路。token 查询失败时仍保留原 `SendInput` 路径，避免把不可判定误报成已拦截。新增纯逻辑回归覆盖“仅高完整性且无 UIAccess 才预检拦截”；`cargo test -p inputflow-windows` 27/27、Clippy `-D warnings` 和 Debug agent build 均通过。

修复版实机结果如下：

- 失败回放：前台跟踪确认 pause 调用前后均为提升目标；报告 `held_events=7`、`inserted_inputs=0`、`requested_inputs=7`、`last_error=5`、`output_complete=false`，随后进入旁路。对应提升目标时间窗共 88 条物理消息，`extra=0x494E5055` 为 0 条。
- 动作输出：临时把 `ui-hold-f8` 改为 `LeftShift / 800 ms / LeftCtrl+C`，从普通窗口启动长按并在阈值前切到提升目标。Agent 记录规则命中后 `cause=uipi_preflight`、`foreground_pid=10380`、`sender_integrity=0x2000`、`target_integrity=0x3000`、`inserted=0/requested=4/last_error=5`，并进入旁路。对应提升目标时间窗共 107 条物理消息，带 InputFlow 标记的消息为 0 条。
- 动作测试的第一次尝试在前台切换前已到 800 ms，因此完整输出 4/4；该轮只作为协调未命中记录，不计入提升目标结论。

证据集中在 `target/acceptance/20261001-013826-physical/uipi-evidence-summary.log`、`normal-target-extra-messages.log`、`elevated-target-extra-messages.log`、`uipi-fixed-replay-foreground-trace.log`、`uipi-fixed-replay-pause-response.json`、`uipi-fixed-action-status-second.json` 和 `uipi-restored-baseline-status.json`。测试后通过正式 UI 将 `ui-hold-f8` 恢复为 `F8 / 500 ms / LeftCtrl+C`；运行时再次为 3 条启用规则、ready/Active、settled。

本轮没有可控、可复现的部分插入来源，因此 partial SendInput **未实机触发，不记通过**；既有故障注入测试继续作为该分支的自动证据。M6-04 的普通完整插入与提升目标 replay/action 零写入通过，partial 保留明确限制。

## 19. 五分钟高负载输入、stats 与资源长稳态

第一次 5 分钟采样运行的是为 UIPI 现场修复临时启动的 Debug Agent。`resources-5min.csv` 共 297 个有效样本、0 错误，callback p99 281 µs、max 3,928 µs，输出失败/丢弃增量均为 0，Hook 持续存活；但 stats RTT p99 为 24.609 ms，超过本轮 20 ms 门槛。这轮如实保留为 Debug 诊断，不用于宣称生产构建通过。

随后构建包含 UIPI 修复的 Release Agent并通过托盘正常退出 Debug 进程，以同一隔离配置启动 Release PID 13952。启动后状态为 ready/Active、settled、3 条启用规则、`last_error=null`。60 秒预检 `resources-release-preflight-60s.csv` 有一次位于第 1 秒的 24.378 ms 冷态尖峰，其后最大 7.516 ms；因此继续执行完整长稳态，并同时保留全样本与去除前 10 秒的统计。

最终证据 `resources-release-5min.csv`：

| 指标 | 全 5 分钟结果 | 稳态（elapsed >= 10 s） | 门槛/结论 |
| --- | ---: | ---: | --- |
| 有效 stats 样本 | 297 | 287 | 两个窗口均 0 采样错误 |
| stats RTT p50 / p95 | 3.670 / 7.224 ms | 3.629 / 7.159 ms | 通过 |
| stats RTT p99 | 16.032 ms | 15.288 ms | `< 20 ms`，通过 |
| stats RTT max | 39.776 ms | 39.776 ms | 单次尖峰保留，不用 p99 隐去 |
| callback 样本增量 | 5,458 | — | 持续有真实 Hook 输入 |
| callback p50 / p95 / p99 | 36 / 98 / 328 µs | — | p99 `< 5 ms`，通过 |
| callback max | 19,387 µs | — | `< 100 ms`，通过 |
| output sent / failed / dropped 增量 | 9 / 0 / 0 | — | 无新增失败或丢弃 |

五分钟内 observed events 增加 4,483。Agent working set 为 11,399,168 → 11,534,336 B（范围 11,399,168–11,563,008 B），private 为 1,925,120 → 2,088,960 B（范围 1,925,120–2,101,248 B）；线程 7 → 7、句柄 161 → 161，未出现持续句柄增长。Settings working set 190,025,728 → 186,126,336 B、private 84,455,424 → 84,332,544 B、线程 21 → 19、句柄 1,134 → 1,130。

结束后的 `resources-release-5min-final-status.json` 仍为 ready/Active、settled、3 条规则、`last_error=null`。用户确认全过程没有可感知的丢键、粘键、鼠标卡住或 Hook 中断。M6-05 与 E-RES-01 通过；100,000 样本排序/热路径互斥边界仍由既有确定性压力测试单列证明，不把本次 5,458 个物理 callback 样本写成 100k 实机输入。

## 20. 左右修饰、扩展键、布局、光标位置与已有物理修饰

增强消息目标 `normal-target-extra-messages.log` 与 Release Agent `--debug-input` 交叉核对了中文（微软拼音）和 English (United States) — US 两个真实布局。中文目标线程收到 `WM_INPUTLANGCHANGE` 后 HKL 为 `0x8040804`，英文为 `0x4090409`。

中文布局下物理单键身份：LeftCtrl scan `0x1D`/非扩展、RightCtrl scan `0x1D`/扩展；LeftAlt scan `0x38`/非扩展、RightAlt scan `0x38`/扩展；LeftShift scan `0x2A`、RightShift scan `0x36`；Left/Right/Up/Down 分别为 scan `0x4B/0x4D/0x48/0x50` 且目标 lParam 扩展位均置位；主 Enter 为 scan `0x1C` 且非扩展。Agent 对所有左右修饰均观察到完整物理 Down/Up。

裸右 Alt 有一个目标消息层边界：中文布局和英文布局的第一次尝试中，Hook 均观察到 RightAlt Down/Up，但目标只收到扩展 `WM_SYSKEYUP`；英文下第二次右 Alt 收到完整扩展 `WM_SYSKEYDOWN/UP`。用户同时观察到裸 Alt 后再按 Enter 会进入 Windows 窗口快捷位置/系统命令操作。这说明物理 Hook 身份没有丢失，但应用目标消息受裸 Alt 系统菜单状态影响；按原始差异记录，不把目标缺少 Down 写成 InputFlow 通过或失败。英文布局的 OEM 实测为分号键 VK `0xBA`/scan `0x27`/字符 `;`，左方括号 VK `0xDB`/scan `0x1A`/字符 `[`。

鼠标位置通过两次 `A → G` 使 `ui-key-mouse = A + Right` 前缀失败并回放：第一组 A/G Down 的 cursor 为 `154,359`，第二组为 `179,748`；两组均保持 A→G FIFO，Down 带 `extra=0x494E5055`，Agent 报告 `inserted=2/requested=2 Complete`。这证明回放使用事件发生时的真实目标/光标环境，且移动鼠标后 Hook/输出继续存活。

已有物理修饰状态场景的首次 F8 尝试恰在 500 ms 阈值前释放，只回放 F8 2/2，不计动作结论。重试时用户实际按住的是 **LeftShift**（不是指令中的 RightShift；目标 scan `0x2A`、Agent `logical=LeftShift`），随后 F8 Hold 命中并完整注入 LeftCtrl Down / C Down / C Up / LeftCtrl Up，最后物理 LeftShift Up 到达；目标顺序严格为物理 Shift Down → 带标记的 Ctrl+C → 物理 Shift Up。已有物理修饰没有被注入动作伪造、提前释放或遗失。RightShift 身份已由前述独立单键测试覆盖，因此本项按“左右身份已覆盖、既有 LeftShift 状态保持通过”记录。

汇总证据见 `target/acceptance/20261001-013826-physical/m6-06-identity-layout-cursor-summary.log`。M6-06 通过；keypad Enter 仍因本机无数字小键盘保持硬件限制，不由主 Enter 或脚本替代。

## 21. 正常退出的两秒 release tombstone 边界

使用正式 `ui-hold-f8 = F8(500 ms) -> LeftCtrl+C`，用户在普通目标中按住 F8 越过阈值，确认动作完整输出 4/4 后保持 F8，并通过托盘 `Exit` 发起正常退出。所有进程均自行结束，没有使用强杀。

首次操作原计划为 ≤2 秒，但用户主观判断释放偏晚；日志明确把它归入 **>2 秒** 分支：

- F8 Hold 命中后持续 repeat，托盘退出使 Hook owner 写出 `shutdown_drain: waiting up to 2000ms for consumed input releases`；
- 两秒内没有 F8 Up，随后写出 `shutdown_limit: consumed input is still physically held after the drain timeout; releases after hook removal cannot be suppressed`；
- 同秒 Agent 正常停止，`hook_panicked=false`、`logger_panicked=false`；
- Hook 移除后普通目标开始收到未带注入标记的物理 F8 repeat，直观证明超时后不再抑制该物理流，也验证了日志所声明的后续 orphan Up 风险。目标因托盘交互失去焦点，最终 Up 没有投递到该特定窗口，因此不虚构目标窗口的 orphan Up 记录。

第二次协调中 F8 Up 发生在托盘命令真正进入 shutdown 之前；进程干净退出且无 limit，但不用于 drain 边界结论。

第三次精确命中 **≤2 秒** 分支：F8 Hold 再次完整输出 4/4；托盘命令交付后先写出 `shutdown_drain`，随后 owner 在线程仍持有 Hook 时观察到 `kbd Up logical=F8 scan=0x42`，同一日志秒内写出正常 `stopped`，没有 `shutdown_limit`。目标只看到此前完整注入 Ctrl+C，没有物理 F8 Down/Up 或孤立 Up，说明 tombstone 在 drain 中平衡消费后才卸载 Hook。

原始摘录见 `target/acceptance/20261001-013826-physical/m6-07-shutdown-boundary-summary.log`。M6-07 通过：两秒内释放可干净排空；超过两秒按明确上限退出并记录后续释放不可抑制的风险。

## 22. Explorer 真重启后的托盘恢复

测试前 Release Agent PID 9088 为 ready/Active、settled、3 条启用规则，Explorer PID 为 5560。用户通过任务管理器对“Windows 资源管理器”执行真实“重新启动”；重启后 Explorer PID 变为 7088、启动时间为 23:57:15，证明不是仅隐藏/显示任务栏。

任务栏恢复后，用户确认 InputFlow 托盘图标自动重新出现，菜单仍显示 Active，`Open Settings / Pause / Exit` 等项目可打开且表现正常。Agent PID 9088 全程未重启；复核 `explorer-restart-final-status.json` 仍为 ready/Active、settled、3 条规则、`last_error=null`。`TaskbarCreated` 后的 tray re-add 实机通过，C-TRAY-01 完成。

## 23. 键盘导航、UI Automation 名称与 Narrator 环境边界

用户仅使用 Tab / Shift+Tab 完成 Settings 的页面导航、按钮与开关遍历，所有预期交互控件均可获得焦点，纯键盘操作符合预期。

启用 Narrator 后，用户观察到部分按钮或开关只能听到控件类型，无法听到有意义的中文名称；本机可能未安装完整中文辅助语音。本项没有把中文文案改成英文来规避测试环境。对运行中窗口进行 UI Automation 枚举，当前页面所有可聚焦控件均具有非空且非通用的 `Name`，包括各规则的启用、编辑、删除按钮；空名称或仅为“按钮/开关”的项目计数为 0。因此应用侧 UIA 名称可验证，但中文语音实际朗读记为 **环境受限、未完成听觉验收**，不虚记通过。

检查时还发现规则行外层 ListItem 的默认 UIA 名称曾是内部类型名 `InputFlow_Settings.MainPage+RuleRow`。修复仅为 `RuleRow` 提供稳定的中文行摘要并由 `ToString()` 暴露，不修改控件模板或尺寸。曾尝试通过局部 `ItemContainerStyle` 移除外层焦点，但该样式覆盖了 ListViewItem 的默认横向拉伸，导致规则卡片不能铺满窗口；该样式已撤销。重建后 UIA 测得“规则列表”和每条规则行的宽度均为 1294 px，行名形如“规则 ui-hold-f8，长按 F8 500 ms，→ Ctrl [LeftCtrl] + C”；用户同时确认视觉布局已恢复。Debug Settings 构建为 0 warning / 0 error。

高对比度测试前系统查询结果为关闭；用户应用 Windows 对比度主题后，`SystemParametersInfo(SPI_GETHIGHCONTRAST)` 返回成功且 `HCF_HIGHCONTRASTON=true`（flags 127），Settings PID 4820 持续响应。用户逐页检查“快捷规则”“设置”“关于”，并打开、滚动和取消“新建规则”编辑器；导航选中态、正文、开关、按钮、状态/诊断、焦点框与编辑器控件均可辨认，没有颜色混淆、截断或滚动障碍，规则行仍正常铺满。高对比度实机通过。

缩放基线最初由用户设为 200%，不是 100%。普通 PowerShell 进程的 `GetDpiForSystem()` 曾因调用方 DPI 感知上下文返回虚拟化的 96 DPI，该值已废弃；后续一律从 WinUI 顶层窗口句柄调用 `GetDpiForWindow()`，并在最终恢复到用户原有 200%。

125% 下窗口实测为 120 DPI。用户遍历三页并把规则编辑器缩到允许的最小宽度；单列切换、纵向滚动、底部操作可达性、文字和控件均符合预期。检查同时发现“设置与诊断”内容根容器被固定为 `MaxWidth=760` 且左对齐，宽窗口下卡片只占左侧一部分。该响应式布局缺陷已修复为 ScrollViewer 内容横向拉伸且不提供水平滚动；按钮继续保持内容宽度。重建为 0 warning / 0 error 后，UIA 在 125%（120 DPI）、窗口宽 1610 px 时测得页面标题宽 1141 px、三组卡片中的标题元素宽 1101 px，证明内容已使用页面可用宽度；用户在放大窗口后确认四个区域均已恢复正常铺满。125% 缩放通过。

150% 下 Settings 顶层窗口实测为 144 DPI；窗口宽 1932 px 时，“设置与诊断”页面标题宽 1367 px，三个主要卡片内容宽均为 1319 px，未回退到固定宽度。用户逐页复验大窗口铺满、文字和控件无重叠/截断、编辑器最小宽度单列与滚动到底、再放大后的布局恢复，所有结果符合预期。150% 缩放通过。

测试结束后用户将显示器恢复为原有 200%。显示器 `GetScaleFactorForMonitor` 返回 200，当前用户 `AppliedDPI=192`；窗口从切换前状态还原后，`GetDpiForWindow` 同样更新为 192。高对比度保持关闭。200% 下 Settings 窗口宽 1753 px，“设置与诊断”页面标题宽 1549 px，三个主要卡片内容宽均为 1485 px；页面同时显示 Agent `ready / 运行中 / 启用规则=3 / reconciliation=settled`。桌面环境和应用运行状态均已恢复。

综上，纯键盘流程、高对比度、125%/150% 缩放、200% 恢复态以及应用侧 UIA 名称均通过；中文 Narrator 实际语音朗读因本机中文辅助语音环境不足保持“环境受限、未完成听觉验收”，不影响已经单独验证的 UIA 名称结论。

## 24. 最终自动回归与验收结论

实机修改完成后重新执行完整门槛：

- `cargo fmt --all -- --check` 通过；仅有沙箱无法 canonicalize 用户目录的非代码警告。
- `cargo test --workspace` 为 151/151：agent 4、config 28、engine 74、protocol 12、runtime 6、windows 27。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo build -p probe-cli`、`cargo build -p inputflow-agent --release` 通过。
- NuGet restore 成功；Settings solution Debug/Release 均为 0 warning / 0 error。
- C# protocol contract 6/6，Settings Core 11/11。

Phase E 的页面录制、三类代表规则、保存/重开、托盘/F12/UI 同步、纯键盘、UIA、高对比度、缩放、资源长稳态和 Explorer 恢复均已有真实 Windows 证据；M6-01～M6-07 现场矩阵也已完成。Phase E 判定为 **完成（含明确限制）**。

保留限制：本机没有 keypad Enter 和独立播放媒体键；中文 Narrator 实际语音因本机语言/语音环境不足未完成听觉判定；partial SendInput 没有可控稳定触发源，仍由故障注入自动测试覆盖而不冒充实机通过。Packaged 启动、干净机安装/升级/卸载/签名以及 x86/ARM64 不属于本轮完成范围。
