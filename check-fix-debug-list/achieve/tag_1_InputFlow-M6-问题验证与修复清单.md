> [!CAUTION]
> 本文件是已废弃的 Tauri 时期历史记录，仅用于问题追溯。
> 不得将其中的前端架构、目录结构或 Tauri 命令作为当前实现要求。
> 当前任务以 `../tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md`
> 和 `../../docs/decisions/ADR-004-Rust常驻Agent与WinUI3设置程序.md` 为准。

# InputFlow：M6 输入可靠性问题验证与修复清单

> 供 DeepSeek 在当前代码仓库中逐项核对、修复和验证。
>
> 本清单基于 **2026-09-27 最初上传的 `input-flow.zip`** 静态检查。用户之后已让 AI 修改代码，因此下列行号和问题状态可能发生变化。先检查最新版本；已修复的项目请给出代码与测试证据，不要机械地重复修改。

## 任务目标与交付方式

InputFlow 的核心承诺是：只有命中规则的物理输入会被消费；未命中的输入应尽量保持原有按下／松开关系、相对顺序和按键身份。请先修正输入正确性，再考虑 M7 GUI。

请按以下步骤工作：

1. 在当前分支记录 `git status` 和当前提交；定位下文提到的逻辑，不以旧版行号作为唯一依据。
2. 对每项问题先给出结论：**仍存在／已修复／需要 Windows 实机验证**，并说明依据。
3. 对仍存在的问题做尽量小且可审查的修改。为状态机加入确定性的事件序列测试；Windows Hook、`SendInput` 与线程排序问题增加适合的集成或手工测试。
4. 运行 `cargo test --workspace` 和 `cargo clippy --workspace --all-targets`，报告实际结果。不要把纯 Rust 单测当作 Windows 实机验证。
5. 给出修改文件、关键设计决定、尚未验证的边界，以及 Windows 上逐项复现的结果。不要声称未执行的测试已通过。

## P0：先解决会改变键盘事件含义的问题

### 1. 键盘松开和扩展键标志读错（用户已实测重现）

- **旧版位置**：`crates/inputflow-windows/src/platform/windows.rs`，`keyboard_proc`，原约第 621–634 行。
- **旧版行为**：从 `KBDLLHOOKSTRUCT.flags` 使用 `KF_UP` 和 `KF_EXTENDED` 判断 `up`、`extended`。这些是另一组位位置；低级 Hook 应检查 `LLKHF_UP` 和 `LLKHF_EXTENDED`，或以正确的 `wParam` 消息种类识别 up/down。
- **实测证据**：用户运行 `--debug` 后发现字母键物理松开仍打印 `DOWN`。
- **影响**：匹配器把松开事件当成再次按下，导致暂扣、长按计时、消费与回放状态错误。
- **验收**：A 的一次按下／松开得到恰好 `Down, Up`；左右修饰键、系统键、注入事件分别验证；纯逻辑测试覆盖低级 Hook 标志位到事件方向和扩展位的转换。

微软文档：<https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct>

### 2. 低级 Hook 没有可直接使用的 `KF_REPEAT` 位

- **旧版位置**：同一 `keyboard_proc` 中 `let repeat = (info.flags & KF_REPEAT) != 0`，原约第 622 行；紧急键判断原约第 654 行。
- **旧版行为**：该表达式不能可靠识别低级 Hook 的自动重复 down。长按 F12 可能反复切换暂停／恢复；规则前缀的重复 down 也可能被当成新的按下。
- **修复要求**：根据维护的物理按键状态判断“此前已按住的再次 down”，明确处理注入事件、暂停和状态重置；不能直接把普通 `WM_KEYDOWN` 消息的重复位套用到 `KBDLLHOOKSTRUCT.flags`。
- **验收**：长按 F12 只切换一次，松开再按才再次切换；长按规则前缀期间重复 down 不重置计时，也不重复触发动作。

## P1：修复未命中输入的原有语义

### 3. `Hold` 类规则期间，其他键直接放行

- **旧版位置**：`crates/inputflow-engine/src/matcher.rs`，`on_key_event` 中 `Some(_) if is_hold_like => self.pass_through(event)`，原约第 335–337 行。
- **确定的事件序列**：启用 `Hold(LeftCtrl, 250ms)+RightButton`，先按 `LeftCtrl↓`（被暂扣），再按 `A↓`（直接放行），随后 `A↑` 和 `LeftCtrl↑`。目标先看到 A，之后才收到 Ctrl 的回放；原本的 `Ctrl+A` 没有得到保留。
- **验收**：加入这一序列的 matcher 测试，明确失败时 Ctrl 和 A 如何按序交付；在记事本已有文字时手动测试 `Ctrl+A` 应全选，`Ctrl+C` 等常用组合也不能被该规则破坏。对 `Hold(K,T)` 单键规则也做同类测试。
- **设计提醒**：解决“是否应使当前候选失败”之外，还必须解决下一项异步输出排序，否则 matcher 返回的数组顺序正确仍不足以保证目标看到的顺序。

### 4. 异步回放与随后直通的物理输入可能乱序

- **旧版位置**：`windows.rs` 的 `process` / `dispatch_command` / `send_output`（原约第 309–364 行），以及 Hook 中返回 `CallNextHookEx` 的路径。
- **事件序列**：`Ctrl↓` 与失败组合的第二键 `Q↓` 被拦截并交给输出线程回放；用户快速松开 Q，`Q↑` 可能先直通，然后输出线程才发送 `Ctrl↓, Q↓`。这会改变按键状态，甚至使目标认为 Q 仍被按住。暂停时冲刷队列也存在类似竞态。
- **验收**：用可控的“延迟输出 worker”构造测试，覆盖失败回放后立即出现的 up、下一组输入、暂停／退出；目标观察到的 down/up 必须成对且相对顺序正确。说明排序协议如何在不长时间阻塞 Hook 回调的条件下生效。
- **注意**：`SendInput` 仅保证一次调用中给定的 `INPUT` 数组按序插入，不能替不同线程和直通路径建立排序屏障。

微软文档：<https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput>

### 5. 输出队列满或断开时，已拦截的事件被直接丢弃

- **旧版位置**：`windows.rs`，`send_output` 对 `TrySendError::Full` / `Disconnected` 只递增 `OUTPUT_DROPPED`，原约第 294–303 行；输出队列容量在 `apps/probe-cli/src/main.rs` 为 8。
- **影响**：Hook 已决定 `Suppress`，随后回放命令丢失；设置旁路也无法自动恢复这批已经拦截的事件。匹配动作丢失同样需要被明确处理。
- **验收**：在测试中故意使输出 worker 暂停并填满队列；检查系统不会默默丢失已暂扣输入，失败状态与日志可诊断。给出明确的背压／排序／旁路设计及其限制，覆盖 `SendInput` 插入数量不足的路径。不要只增大队列容量。

## P2：状态与按键身份边界

### 6. 暂停时清空“已消费”状态，之后松键可能孤立直通

- **旧版位置**：`matcher.rs`，`set_paused(true)` 调用 `keys.clear()` 和 `buttons.clear()`，原约第 295–302 行；`windows.rs` 的 `suspend()` 立即进入旁路。
- **事件序列**：`Ctrl(Hold)+RightButton` 命中，物理 Ctrl 和右键仍按住、其 down 已消费；此时触发暂停；之后才释放 Ctrl／右键。原先应消费的 up 可能交给目标，但目标没有见过对应 down。
- **验收**：事件序列测试同时覆盖键盘与鼠标；明确暂停时在途输入和已消费按键的归属，避免孤立 up，并避免为了处理 up 而继续长时间拦截所有输入。

### 7. 回放没有完整使用扫描码与扩展键信息

- **旧版位置**：`windows.rs`，`event_to_input` / `make_key_input`，原约第 405–437 行；`InputEvent` 已存 `scan_code` 和 `extended`，但生成的 `KEYBDINPUT` 只设置了 `KEYEVENTF_KEYUP`，未处理 `KEYEVENTF_SCANCODE`、`KEYEVENTF_EXTENDEDKEY`。
- **影响**：按键身份与原物理事件可能不完全相同，尤其应关注左右 Ctrl/Alt、扩展键和不同键盘布局。此项的具体表现需 Windows 实机验证。
- **验收**：测试左右修饰键的识别、失败回放与目标侧观察结果；如果选择 VK 回放而非扫描码回放，说明它能保留哪些语义、哪些仍有限制。

微软文档：<https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput>

## 建议的 Windows 人工验收顺序

在普通权限的记事本中，先备好若干文字，并保留退出程序的控制台窗口。每次只启用一条规则；逐项记录配置、物理输入顺序、`--debug` 日志、记事本中的实际效果和 `stats`／退出计数。

1. **空规则探针**：字母和鼠标按钮的 down/up 成对；F12 单次短按、长按行为正确。
2. **演示规则** `Hold(LeftCtrl,250ms)+RightButton → Ctrl+C`：Ctrl 达到阈值后右键命中一次；原右键菜单不出现；释放 Ctrl/右键后无卡键。
3. **失败回放**：快速按下并松开 Ctrl+A，应保持全选；Ctrl+C、Ctrl+Q 等普通输入应保持其原有组合语义。短按 Ctrl 单独松开后没有残留按住状态。
4. **并发边界**：失败组合后立刻松键、接着打字；暂扣期间暂停／退出；命中后保持触发键按住再暂停、随后松键。
5. **失败路径**：人为放慢输出 worker 以触发队列满；检查 `output_dropped`、日志及输入效果。提升权限窗口中的 `SendInput` 行为单独记录，不能用普通窗口的结果代替。

## 最终回复模板

请以表格报告每个编号的最新状态、修改文件、自动测试、Windows 实机结果；最后列出尚未解决的限制。若最新代码已经解决某项，请直接引用相应实现和覆盖该事件序列的测试。仅在上述输入正确性问题验证完成后，才建议开始 M7 图形界面。
