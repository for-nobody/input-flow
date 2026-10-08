> [!CAUTION]
> M6 历史任务，仅用于问题追溯。Tauri 架构、旧路径和任务状态均已失效；当前状态见
> [`../../status/CURRENT_STATUS.md`](../../status/CURRENT_STATUS.md)。

# InputFlow M6：第二轮复查与修复任务

> 请在当前 InputFlow 仓库中核对并处理本清单。本清单基于 **`input-flow(1).zip`（2026-09-27 02:44 上传版本）** 的静态复查；后续代码可能已经变化。先定位最新实现，再判断问题是否仍存在，不要依据旧行号盲目修改。
>
> 目标：在开始 M7 前端界面之前，让输入捕获、暂扣、回放、消费、暂停和配置保存具有可验证的行为。

## 工作方式

1. 先记录当前分支、提交和 `git status`，不要覆盖用户已有改动。
2. 逐项给出“已修复／仍存在／需要 Windows 实机验证”的结论，引用当前代码位置和证据。
3. 对仍存在的问题实施小范围修复；增加能复现失败的测试，避免仅测试新增函数本身。
4. 运行 `cargo test --workspace`、`cargo clippy --workspace --all-targets`，如可行，再做 Windows 实机输入测试。列出实际命令与结果，不要把纯逻辑测试说成真实 Hook 测试。
5. 提交修改文件、关键设计取舍、每个事件序列的预期与实际结果、未解决限制。若 `SendInput` 因 UIPI 等原因无法送达，请明确承认原输入无法保证恢复，并提供可诊断的失败处理。

## 上一轮七项问题：新版静态复查状态

| 编号 | 问题 | `input-flow(1).zip` 状态 | 本轮应验证的内容 |
| --- | --- | --- | --- |
| 1 | `KBDLLHOOKSTRUCT.flags` 错用 `KF_UP`／`KF_EXTENDED` | 已改用 `LLKHF_UP`／`LLKHF_EXTENDED`，并新增位标志测试 | Windows 上按下与松开 A 应分别记录 Down、Up；测试左右修饰键 |
| 2 | 将 `KF_REPEAT` 用作低级 Hook 重复标志 | 已改用 `HELD_KEYS` 推断重复 down | 长按 F12 只能切换一次；松开后再按才能再次切换；检查第三方注入事件是否污染“物理按住”集合 |
| 3 | `Hold` 前缀暂扣 Ctrl，却让 Ctrl+A 的 A 直接放行 | matcher 已在其他键 down 时使候选失败，新增事件序列测试 | 在记事本测试普通 Ctrl+A／Ctrl+C；同时验证输出线程能维持实际顺序 |
| 4 | 异步回放被后续物理 up 超越 | 新增确认通道，但**仍有确认超时及错配漏洞** | 见问题 A |
| 5 | 满输出队列直接丢弃已抑制事件 | 改为阻塞 `SyncSender::send`，但**Hook 可能无限期卡住** | 见问题 B |
| 6 | 暂停清空已消费状态，造成孤立 up | 增加 synthetic down，但**可能恢复已消费的鼠标点击** | 见问题 C |
| 7 | 回放丢失扩展键信息 | 已设置 `KEYEVENTF_EXTENDEDKEY`；仍采用 VK 回放，`wScan` 并非识别依据 | Windows 实测左右 Ctrl／Alt、扩展键、不同布局；明确支持边界 |

## 必须在 M7 前解决

### A. 输出确认超时后会读到上一条命令的确认（P0）

- **位置**：`crates/inputflow-windows/src/platform/windows.rs` 的 `wait_output_ack()`、`dispatch_command()`；`apps/probe-cli/src/main.rs` 的 `output_loop()`。
- **现状**：Hook 等待 `Receiver<()>` 最多 50 ms，超时结果被忽略；worker 在 `execute()` 和打印日志后发送没有命令编号的 `()`。
- **确定的竞态**：命令 C1 在 50 ms 内未执行完，第一次等待超时；C1 在 70 ms 完成，确认进入通道；C2 随后入队，第二次等待立刻消费 C1 的旧确认。此时 C2 尚未插入输入流，但 Hook 已允许下一事件继续前进。
- **影响**：所谓“排序屏障”失效。确认还没有区分 `SendInput` 完整成功、部分成功、失败。
- **验收**：以可控制的 worker 延迟（例如 C1 延迟 70 ms）构造测试；C2 绝不能用 C1 的确认完成。确认需要对应特定命令及执行结果。超时必须改变状态并触发明确的恢复／旁路策略，不能静默继续并把后续输入当作已排序。快速按下、松开失败组合时，目标看到的 down/up 顺序正确。
- **设计要求**：说明在 `SendInput` 延迟、失败和工作线程退出时，已暂扣输入分别由谁持有、如何处理；不要仅把 50 ms 调大。

### B. 阻塞发送发生在低级 Hook 路径（P0）

- **位置**：`windows.rs` 的 `send_output()` 使用 `SyncSender::send()`；`process()`／`dispatch_command()` 可由键盘／鼠标 Hook 回调调用。
- **现状**：输出队列容量为 8；worker 停滞且队列满时，`send()` 可以无限期等待。注释中的“由 worker 排空速度限定”不等于有时间上限。`process()` 在派发时还持有 matcher 锁。F12 也依靠相同 Hook 线程处理，因此可能无法及时触发紧急旁路。
- **验收**：让 worker 故意停止消费并填满队列，测量 Hook 回调耗时；不得无限等待，也不能静默丢掉已抑制事件。给出可执行的有界队列溢出／失败协议，并测试断开的 worker。记录 p95/p99 与最大回调耗时。
- **补充**：当前 `record_callback_latency()` 在派发和等待之前调用，统计值没有包含这段阻塞。修复时应让指标反映用户实际承担的回调等待成本。

微软低级 Hook 文档说明，超时后 Hook 可能被系统静默移除：<https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc>

### C. 暂停时补发已消费的右键 down，可能重新触发点击（P1）

- **位置**：`crates/inputflow-engine/src/matcher.rs` 的 `set_paused(true)`，以及 `windows.rs` 的 `suspend()`／旁路逻辑。
- **事件序列**：`Hold(LeftCtrl,250ms)+RightButton → Ctrl+C` 命中，Ctrl 和右键仍物理按住；此时按 F12 暂停；matcher 回放 synthetic `Ctrl↓, RightButton↓`，之后物理 `RightButton↑` 直接放行。
- **风险**：原本被规则消费的右键动作可能在暂停后形成一次右键点击／菜单；synthetic down 也可能发生在已经移动后的鼠标位置。解决孤立 up 不应无条件让已消费的触发动作复活。
- **验收**：在记事本或可记录鼠标 down/up 的测试窗口执行该序列，确认规则动作只发生一次、原右键菜单不出现、目标不收到孤立 up。键盘与鼠标按键分别覆盖；定义暂停过渡期间如何处理已消费键的释放。

## 随本轮一并处理或明确限制

### D. 已暂扣键的自动重复 down 被丢弃

- **位置**：`matcher.rs` 的 `on_first_key_event()`；新版遇到 `event.is_repeat()` 时返回 `Suppress/Pending`，不把事件放入回放队列。
- **复现**：设置较长的 `Hold(A,T)+Button`（T 大于系统自动重复启动时间），按住 A 至重复开始，再按一个不匹配按钮使规则失败；用户原本的 A 自动重复应得到保留，而当前只会回放初始 A down。
- **验收**：失败回放包含需要保留的重复输入，同时有界队列不能被无上限重复事件耗尽。若 MVP 有意不支持“可重复键作为长按前缀”，应在配置验证阶段明确拒绝，而非静默丢键。

### E. GUI 即将调用的配置保存并非故障安全

- **位置**：`crates/inputflow-config/src/config.rs` 的 `save()`：第一次重命名失败时，先删除旧配置，再把 `.tmp` 重命名为正式文件。
- **风险**：如果第二次重命名失败，旧配置已被删掉。固定 `.tmp` 路径也需要考虑重入或并发保存。此问题在 GUI 可以频繁编辑规则后更容易造成用户可见的数据丢失。
- **验收**：模拟替换失败，确认旧配置仍可读取；保存成功后新配置可读取且版本正确。采用适合 Windows 的替换／备份策略，并准确描述其原子性与剩余风险。

Windows 文件替换参考：<https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew>

### F. 键盘与鼠标回放身份仍需实机证明

- 键盘回放现在保留 `KEYEVENTF_EXTENDEDKEY`，但 `KEYEVENTF_SCANCODE` 未使用。请实测左右 Ctrl、Alt、Shift，说明 VK 方案在不同键盘布局下能保留的语义。
- `event_to_input()` 丢弃鼠标事件保存的坐标，`make_mouse_input()` 只在当前指针位置发送按钮。如果暂扣期间鼠标移动，回放点击可能发生在新位置。请测试并明确所需位置语义；若项目承诺“未命中输入保持原行为”，应处理此差异。
- 这些行为是否在常用配置下产生问题，不能仅凭纯逻辑单元测试得出结论。

`SendInput` 只保证同一次调用中 `INPUT` 数组的连续插入，并受 UIPI 限制：<https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput>

## 最小 Windows 手工回归

在普通权限的记事本和一个能够记录鼠标按键的测试窗口中逐项验证；每项记录配置、物理事件顺序、`--debug` 日志、实际 UI 效果、输出计数和 `stats`：

1. 空规则下 A、左右 Ctrl、左右鼠标键分别得到 Down、Up；长按 F12 仅切换一次。
2. 演示规则启用时，快速 Ctrl+A 应全选；Ctrl+C 和 Ctrl+Q 保持原行为。
3. 长按 Ctrl 达阈值后按右键，动作只执行一次，原右键菜单不出现，松开后无残留按住状态。
4. 命中规则但保持 Ctrl／右键按住，触发暂停，再松开两键；无额外菜单或孤立 up。
5. 对输出 worker 注入大于 50 ms 的延迟、填满队列、停止 worker；检查命令确认、Hook 耗时、紧急旁路和按键归属。
6. 失败组合后立刻松开第二键并快速输入下一键；检查目标实际收到的顺序。
7. 记录 `SendInput` 在普通窗口和提升权限窗口中的实际返回值。若注入被拒绝，不宣称已暂扣输入得到完整恢复。

## 完成标准

请先报告 A、B、C 的代码修复和故障注入测试，再报告 D、E、F 的处理或明确限制。最后提供自动测试与 Windows 手工回归的真实结果。**只有输入可靠性门槛达到并记录剩余限制后，才开始 M7 GUI。**
