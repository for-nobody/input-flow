> [!CAUTION]
> M6 历史任务，仅用于问题追溯。Tauri 架构、旧路径和任务状态均已失效；当前状态见
> [`../../status/CURRENT_STATUS.md`](../../status/CURRENT_STATUS.md)。

# InputFlow M6 三轮复查与 Codex 系统修复任务

> 交给 VS Code Codex 的任务说明。请以仓库当前实际代码为准，先复现、分析和设计，再修改与验证。记录做了什么、为什么、哪些现象已经实测，切勿把假设写成事实。

## 0. 背景、范围与证据级别

- 复查对象：用户第三次提交的 input-flow(2).zip，Git HEAD 为 a7aff2d（基于 f1491a9 的第二轮修复）。
- 架构目标：Windows 低级键鼠 Hook + 输入暂扣/匹配/回放；输入失败时优先保持原生行为；后续由 Tauri 2 + React UI 配置规则。
- 本轮已确认：旧输出 worker、命令队列、无标识 ack 已移除；输出由 Hook/定时器路径同步调用 SendInput。旧 ack 错配、阻塞的 SyncSender::send、队列满丢输出命令这三条特定代码路径已不存在。勿盲目恢复原方案。
- DeepSeek 报告 cargo test --workspace：67 通过、0 失败；cargo clippy --workspace --all-targets：无警告。复查者数到了 67 个测试定义，但复查环境为 Linux 且没有 Cargo/Windows，**未独立执行**这些命令、没有实测 Win32 Hook。下面的“确定”均指代码路径静态可证，实际桌面效果仍须 Windows 验证。
- 文件行号以 a7aff2d 为准；修改后请重新定位。把确定缺陷、可触发的故障路径、风险与已接受的产品限制分开记录。

## 1. 工作方法和交付要求

请按此顺序处理，不要直接跳到“全部修好”：

1. 确认当前提交、工作区状态，阅读项目规划、README、相关 ADR 和本清单。先列出每项的复现输入、预期输出、实际输出和证据来源。
2. 对每项列至少一个原因假设，画出事件顺序和状态变化；检查假设能否由纯 matcher 测试或可注入的平台接口验证。区分“Hook 决策正确”和“目标程序最终收到正确输入”。
3. 提出候选方案，讨论输入顺序、最坏回调耗时、失败恢复、Windows 消息语义、复杂度和现有规则兼容性。若原定 fail-open 在某些 Win32 权限边界内无法严格实现，要明确边界及产品策略，不得把“已设置 BYPASS”称为“先前被拦截的输入已恢复”。
4. 先添加能揭示问题的回归测试/故障注入，再做有针对性的改动。保留上一轮有效修复，避免大范围重构掩盖问题。
5. 运行格式化、cargo test --workspace、cargo clippy --workspace --all-targets；在 Windows 实机执行本文的手工场景，并记录命令、系统环境、观察结果及失败日志。无法运行的项目要明确写“未验证”，不得编造通过结果。
6. 形成新的修复记录 MD：问题编号、根因证据、选定方案与放弃方案的理由、改动文件、测试及输出、剩余限制、是否达到进入 UI 集成的门槛。尽量避免只给“测试通过”的笼统结论。

优先处理 §2 的输入正确性，再处理 §3 的可靠性。无需在这轮编写 Tauri/React UI。

## 2. 输入正确性：进入 UI 前必须解决

### IF-01：暂停后放行“已消费按下”的右键松开，可能产生原本应被消费的菜单

**确定的代码路径**

- crates/inputflow-engine/src/matcher.rs：set_paused() 约 295–307 行清空 consumed 状态，并注释断言孤立 Up 无害。测试 pause_does_not_replay_consumed_inputs 只验证后续 Right Up 与 Ctrl Up 得到 PassThrough；没有检查目标窗口效果。
- crates/inputflow-windows/src/platform/windows.rs：keyboard_proc/mouse_proc 在 is_bypassed() 时直接 CallNextHookEx（约 712–715、762–765 行），不保留此前已消费 Down 与其 Up 的配对信息。
- Windows 官方文档说明 DefWindowProc 在处理 WM_RBUTTONUP 时生成 WM_CONTEXTMENU。因此“目标应用总会忽略未见 Down 的 Up”**没有依据**；至少右键菜单有明确反例风险。

**Windows 实机复现**

1. 使用示例规则 Hold(LeftCtrl, 250ms)+RightButton → Ctrl+C，使能规则，并聚焦一个能观察 WM_CONTEXTMENU 或显示右键菜单的普通窗口。
2. 按住左 Ctrl 超过阈值，按下右键，确认规则命中、Right Down 已被消费；保持右键物理按住。
3. 按 F12 进入紧急旁路（不要切走目标窗口），随后松开右键和 Ctrl。
4. 观察目标窗口是否收到 Right Up / WM_CONTEXTMENU，是否显示菜单。另做 KeyMouseButton、普通左键及命中后在控制台 pause 的变体。
5. 纯 matcher 回归测试至少断言暂停时不同类别输入的逻辑命运；真实菜单效果必须在 Windows 上验证，不能由 Decision::PassThrough 测试替代。

**设计要求 / 验收**

- 先定义“pending 仍在队列的 Down”和“已经命中、Down 被消费且物理仍按住”两种状态的不同退出策略。
- 暂停、紧急旁路、输出失败、正常退出和恢复后，不凭空回放已消费 Down，不放行可能触发用户可见操作的孤立 Up；不能卡住键鼠的状态。考虑在旁路期间保留极小的配对/墓碑状态，直到物理 Up；同时说明这样做与“旁路立即不拦截任何输入”的语义取舍。
- 给出事件序列测试与实机窗口消息日志。若确有无法完全避免的 Win32 场景，记录可观察影响和限制。

### IF-02：规则前缀的自动重复仍被丢弃；第二轮验证只覆盖部分规则

**确定的代码路径**

- crates/inputflow-engine/src/matcher.rs：on_first_key_event() 约 480–490 行对于任何活动前缀的 repeat Down 返回 Suppress + Pending，且不保存重复事件；该函数同时服务 Hold、HoldMouseButton 和 Chording。
- crates/inputflow-config/src/config.rs：check_hold_prefix_key() 仅用于 Hold 和 HoldMouseButton（约 241–270 行）；KeyChord、KeyMouseButton 可以使用 A 等自动重复键作为首键。
- crates/inputflow-engine/src/event.rs：Key::auto_repeats() 约 263–311 行枚举字母、数字等，但不含 F1–F24；现有 matcher 测试 repeat_down_does_not_reset_the_timer() 恰好模拟了 F8 的重复 Down。该分类与测试的输入模型不一致。

**最小可复现序列**

- 规则 KeyChord(A, B) → C；A Down（暂扣）、A Repeat Down × N（被拦截但未保存）、A Up（失败，仅回放初次 A Down + A Up）。普通文本窗口原应收到保持按键期间的重复输入，实际只可能收到一次 A。
- KeyMouseButton(A, Right) 也走首键重复路径。另测试 Hold(F8, T) 在 T 前产生重复并释放；配置当前允许 F8，但重复被吞掉。F8 的实际硬件/系统重复行为须在 Windows 实机确认，纯代码测试已能证明“若出现重复就会丢”。

**设计要求 / 验收**

- 选择并明确产品语义：保存并在失败时按序回放重复事件；或者限制所有可能自动重复的规则前缀并通过 Windows 行为验证分类。若匹配成功时有意消费重复，要在规则语义中解释。不能仅对 Hold/A 增加一个配置拒绝用例。
- 覆盖 KeyChord、KeyMouseButton、Hold、HoldMouseButton；测试“重复后失败”“重复后命中”“重复后暂停/溢出”，保持队列有界且无静默丢失。优先给普通字母、数字、功能键和修饰键的 Windows 实测矩阵。
- 如果暂扣队列容量不足以保存长时间重复，定义可测试的降级/旁路与回放方式。

### IF-03：SendInput 失败时设置 BYPASS，并没有恢复本次已被压制的事件

**确定的代码路径**

- crates/inputflow-windows/src/platform/windows.rs：execute() 约 279–301 行检查已插入数量，不足则设置 BYPASS；dispatch_command() 约 370–381 行不把失败结果反馈给 Hook 决策；keyboard_proc/mouse_proc 约 718–724、767–772 行仍按匹配器的 Suppress 返回 1。
- 对 Failed { replay }，原先暂扣的事件已从 pending 中取出；若 SendInput 插入 0 或部分事件，完整原始输入可能永久丢失。对 Matched { action }，动作可能只执行一半。BYPASS 只保护**之后**到来的输入。微软文档指出 SendInput 可返回小于请求数，且 UIPI 阻挡并不能从返回值或 GetLastError 精确识别。

**确定性复现方式（优先）**

1. 抽象可注入的输出调用，在测试中强制返回 0/部分数量；执行“Ctrl 前缀 + 其他按键触发失败回放”“Hold 到期触发动作”“匹配和暂停冲刷”三类序列。
2. 记录实际插入数组、Hook 的 Suppress/PassThrough、本次暂扣事件状态、BYPASS 状态。特别验证部分插入的修饰键是否造成不成对输入。
3. 实机尝试普通权限进程与更高完整性窗口等失败条件，但这种环境未必稳定触发；故障注入测试不能省略。

**设计要求 / 验收**

- 明确区分“未暂扣的当前事件，可以在返回前放行”“以前暂扣的事件，只有成功回放或主动限制拦截范围才可能恢复”“部分插入造成的状态不一致”。禁止宣称“设置 BYPASS 就完全 fail-open”。
- 设计可解释、可测试的权限/目标窗口策略和故障恢复路径；若系统拒绝注入导致先前事件客观无法恢复，必须降低触发概率、报告限制并保留紧急退出能力。记录输入丢失和动作部分执行的残余风险。
- 输出计数、失败日志和测试应区分零插入、部分插入、完整插入；不得仅以 SendInput 返回完整数量推断目标应用已执行动作。

## 3. 运行可靠性：接入 GUI 前处理

### IF-04：SetTimer 安装失败仍宣布 Hook 可用，Hold 规则可能一直等不到超时

**确定的代码路径**

- crates/inputflow-windows/src/platform/windows.rs：run_hook_thread() 约 540 行先向主线程报告 ready；约 552–565 行才调用 SetTimer。返回 0 时只写日志，仍进入消息循环。
- Hold/ HoldMouseButton 到期依赖 run_message_loop() 中的 WM_TIMER → drive_timeouts()。若定时器未建立，用户单独按住前缀键可能长期暂扣，HoldMouseButton 永不进入 armed 状态；虽然松开或其他按键可能触发失败回放，不等于定时规则正常工作。

**复现 / 验收**

- 为定时器安装设置故障注入点，强制失败，检查启动 ready 状态、规则是否启用、输入是否仍会被暂扣、错误是否明显可见。
- 定时规则不能在缺少驱动器时照常启用；可选择在启动失败时卸载 Hook/拒绝启动，或只运行可靠的无时序旁路路径。实际处理取决于设计方案，但需保证按住前缀不会无限等待。
- 测试消息循环繁忙时的到期延迟；不能用“SetTimer(5ms)”推断实际每 5ms 得到处理。

### IF-05：配置保存的备份/恢复流程在崩溃及恢复失败时仍可能失去正式配置

**确定的代码路径**

- crates/inputflow-config/src/config.rs：save() 约 327–376 行先写 config.json.tmp.PID，再 rename(config.json→config.json.bak.PID)，接着 rename(tmp→config.json)。中间若进程崩溃，正式路径缺失。
- 如果第二次 rename 失败，恢复 rename(backup→config.json) 的结果被忽略。重启时 load() 对正式文件缺失使用空规则回退，并没有从 .bak.PID 自动恢复。
- “唯一”临时名实际仅由 PID 决定，单进程并发 save 会共用名字；写入用 File::create，可覆盖残留临时文件。保存前无专门故障注入或并发测试；现有 save_overwrites_existing_config 只验证正常覆盖。

**复现 / 验收**

- 故障注入：在两个 rename 之间终止/模拟异常；在最终 rename 失败后，再强制恢复失败；并发调用 save。检查文件实际内容、load() 结果和目录中备份，不能只断言 save() 返回 Err。
- 提出 Windows 可用的替换方案（例如审视 ReplaceFileW 的备份和错误条件），并设计启动恢复逻辑；首次创建正式文件、磁盘空间不足、权限/占用失败、进程崩溃分别考虑。
- 验收目标：旧配置与新配置至少有一份可被程序明确发现并恢复；失败信息不能被静默吞掉；不会覆盖仍可能是唯一有效副本的备份。切勿把普通双 rename 称为无条件原子写入。

### IF-06：同步 SendInput 解决了旧队列问题，但引入 Hook 耗时风险，尚无 Windows 性能证明

**证据与性质：性能/架构风险，暂未实测定论**

- crates/inputflow-windows/src/platform/windows.rs：process() 约 321–362 行持有 Matcher 锁时在 dispatch_command() 内同步调用 SendInput、格式化日志、更新采样；drive_timeouts() 也在持锁时执行输出。注释称 SendInput “bounded”，但 API 返回插入数量并没有提供 callback 的最坏耗时保证。
- record_callback_latency() 现包含同步输出，但只测 process()，没有计入回调标准化、前置日志、锁前处理及 CallNextHookEx。报告中的指标不是完整 Hook 回调耗时。
- Microsoft 官方文档要求低级 Hook 快速返回；超时的 Hook 可被系统静默移除。把输出搬回 Hook 是一个取舍，不能仅因没有 output worker 就判定时延可靠。

**验证 / 决策**

- Windows 实机测连续打字、自动重复、鼠标快速点击、多条规则、高负载、SendInput 插入 0/部分成功情况下的 p50/p95/p99、最大耗时、事件顺序、Hook 仍在工作情况；如可行，使用外部窗口事件记录独立对照。
- 评估是否应调整锁范围、同步输出数量、故障恢复策略或架构；任何改为异步输出的方案都必须重新证明回放不会被随后原始事件超车，且绝不在 Hook 中无限等待队列/ack。
- 明确可接受阈值及异常时行为，不用 Cargo 单测替代 Windows Hook 的性能验收。

## 4. 已知行为与较低优先级验证

这些事项不是本清单新增“已证实必修复”结论；在接入允许用户选择任意输入的 UI 之前，需要实机验收或明确限制：

- 键盘回放：windows.rs 的 make_key_input() 约 469–479 行写入 wScan，但未置 KEYEVENTF_SCANCODE；回放仍以虚拟键为主，跨键盘布局、左右修饰键、扩展键需要真实布局和应用验证。
- 鼠标回放：event_to_input() 约 422–445 行忽略已记录的 x/y；按钮回放发生在当前光标位置，鼠标移动期间的失败/暂停场景可能改变目标。决定支持的回放语义，若选择保留限制应向 UI 用户说明。
- 动作与物理修饰键：SendInput 不会自动重置当前按键状态；已有其他修饰键按下时，KeyChord action 的目标效果可能变化。用 Windows 真实目标程序验证，不凭抽象测试宣称快捷键完全隔离。
- 输出失败后 BYPASS、F12 紧急切换、暂停/恢复、退出冲刷之间可能交错；覆盖“按住期间切换”和重复快速切换，检查 Up/Down 配对。

## 5. 建议的最小回归矩阵

| 场景 | 必须观察的结果 | 证据方式 |
| --- | --- | --- |
| KeyChord(A,B) 的 A 重复多次后松开 | 原始重复次数/顺序按选定语义处理，未静默丢弃 | matcher 测试 + Windows 文本框 |
| KeyMouseButton(A,Right) 的 A 重复后失败 | 失败回放保持顺序和重复信息 | matcher 测试 + 实机记录 |
| Hold(F8,T) 在阈值前重复并释放 | 配置拒绝或完整回放，行为与声明一致 | config/matcher 测试 + 实机确认 F8 |
| 已消费右键 Down → F12/pause → 物理右键 Up | 不出现非预期 WM_CONTEXTMENU/菜单；状态可恢复 | matcher 测试 + Windows 窗口消息记录 |
| SendInput 返回 0 / 部分数量 | 不误报“完全回放”；后续输入放行；明确先前输入损失或恢复方案 | 故障注入 + 实机边界 |
| SetTimer 安装失败 | 不启用无法到期的 Hold 行为，启动状态准确 | 故障注入 |
| 配置保存两个 rename 中途崩溃、恢复 rename 失败 | 旧或新配置可发现并恢复；错误可诊断 | 文件系统故障注入/重启测试 |
| 持续输入及高负载回放 | 延迟阈值符合决策，Hook 未失效，输出未超车 | Windows 实机性能与窗口日志 |

## 6. 参考资料（微软官方）

- WM_CONTEXTMENU：DefWindowProc 对 WM_RBUTTONUP 的处理  
  https://learn.microsoft.com/en-us/windows/win32/menurc/wm-contextmenu
- SendInput：插入数量、UIPI、序列化及已有按键状态  
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- LowLevelKeyboardProc：超时与 Hook 静默移除  
  https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
- SetTimer：失败返回 0、WM_TIMER 的生成规则  
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-settimer
- ReplaceFileW：替换、备份与错误状态  
  https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew

## 7. 完成判定

请先提交逐项的“复现 → 根因 → 方案比较 → 代码改动 → 自动测试 → Windows 实测 → 剩余风险”记录。只有当 IF-01 至 IF-05 的代码问题被实质解决、IF-06 的时延与 Hook 存活得到实机验收、较低优先级行为有明确支持范围时，才把输入引擎标记为可接入会让用户创建/启用规则的正式 UI。视觉设计可以独立推进。
