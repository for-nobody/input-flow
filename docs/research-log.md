# InputFlow 研究日志（research-log.md）

> 记录四条主线的调研结论，不堆功能数量。未经实机验证的结论明确标注“待验证”，不能当作已实现能力。
> 研究主线：1) Windows 输入链路；2) 已有工具；3) 算法；4) 交互。

## 1. Windows 输入链路（Hooks / Raw Input / SendInput / HID）

- 采用 `WH_KEYBOARD_LL` + `WH_MOUSE_LL` 低级 Hook 观察并选择性抑制键盘/鼠标事件；Hook 是否阻止当前事件必须在回调返回前决定，因此决策必须在回调内轻量、同步完成（参见 LowLevelKeyboardProc / LowLevelMouseProc）。
- Hook 回调必须快速返回，否则可能被系统静默移除；需专用消息循环线程，回调内只做归一化 + 轻量前缀/状态判定 + 有界入队。
- 回放与动作输出用 `SendInput`，需检查实际插入数量并处理 UIPI 完整性级别限制；`SendInput` 不会重置现有键盘状态，组合输出需考虑物理修饰键已按住的情况。
- `KBDLLHOOKSTRUCT` / `MSLLHOOKSTRUCT` 提供 injected 标志与 `dwExtraInfo`，用于区分本程序生成的事件。
- Raw Input 用于“设备来源”的后续研究（M8）；MVP 不用它承担抑制功能。
- 待验证：Hook 超时阈值、`SendInput` 在提升权限窗口/前台切换下的实际行为、与输入法及其他改键软件共存。

## 2. 已有工具（Kanata / KMonad / AutoHotkey / PowerToys / reWASD）

- 竞品验证了“低级 Hook + 合成输入 + 前缀暂扣/回放”的整体可行性，也暴露了焦点变化、权限、自触发、状态一致等共性风险。
- 关注点：竞品如何捕获输入、解析歧义、处理回放/失败策略，以及用户需要理解的概念；不以功能数量替代设计分析。
- 待验证：本项目“物理按住 vs 目标已看到”的双状态模型与竞品的差异是否带来更好的顺序/状态一致性。

## 3. 算法（有限状态机 / 前缀匹配 / 超时与队列）

- 核心匹配器与 Windows 解耦：纯逻辑状态机 + 可注入逻辑时钟，测试不依赖真实睡眠。
- 用预编译规则把复杂匹配变成回调可快速查询的形态；只暂扣可能构成已启用规则前缀的事件。
- 暂扣队列有界；超时/失败按序回放；每个物理 down 对应的 up 必须有明确归属，防止孤立释放。
- 待验证：多候选竞争、自动重复 down、双击、溢出、计时器重复触发等边界（M3 起用事件序列单测覆盖）。

## 4. 交互（录制规则 / 冲突提示 / 旁路与延迟反馈）

- MVP 先用表单编辑规则（录制不稳定之前）；后续再评估录制器。
- 冲突规则先拒绝并解释原因，不暗中选优先级（MVP）。
- 旁路/暂停要可见、可一键恢复；延迟反馈要能提示“可能造成明显输入延迟”。
- 待验证：用户能否理解“暂扣/回放/消费”造成的可观察行为差异（丢键/卡键/菜单不弹出）。

## 5. M1 实机观察（2026-09-26，Windows 11 build 26200）

- `WH_KEYBOARD_LL` + `WH_MOUSE_LL` 在专用消息循环线程上安装成功；回调内只做归一化 + 非阻塞 `try_send` 到有界队列，由独立 logger 线程打印（符合 Microsoft「把工作交给工作线程并立即返回」的建议，也满足 NFR-01）。
- 实测捕获物理鼠标滚轮、左右键按下/松开：`injected=false`、带系统 tick 时间戳（`MSLLHOOKSTRUCT.time`，ms）、坐标、`dwExtraInfo=0`，且 `seq` 单调递增、顺序正确。
- 退出路径验证：控制台输入 `quit`/`exit`/`q` → `PostThreadMessageW(WM_QUIT)` → `GetMessageW` 返回 0 → 卸载两个 Hook → logger 冲刷剩余事件 → 退出码 0，无残留。确认 `PostThreadMessage(WM_QUIT)` 对已创建消息队列的线程有效。
- 修复一个死锁：logger 线程若长期持有 `io::stdout().lock()`，主线程打印启动横幅时会永久阻塞；改为「每次写入时短暂持锁」解决。
- 待验证（后续手动实机）：记事本/浏览器逐键打字顺序、左右 Ctrl 的键盘事件（本次实测主要为鼠标事件）、与输入法/其他改键软件共存、Hook 超时阈值。

### M2 方向（依据 M1 观察）

- 物理输入 `dwExtraInfo=0`；M2 注入时写入本程序唯一标记，并结合 `injected` 标志即可区分本程序生成的事件、避免自触发递归。
- 回调「归一化 + 有界入队」路径可复用；M2 只需在回调中对 F8 down 返回非零以抑制，并在工作线程用 `SendInput` 延迟回放，同时检查实际插入数量。
- 需实机验证：`SendInput` 返回不足、提升权限窗口、回放事件再次进入 Hook 时的识别。

## 6. M2 抑制与回放（2026-09-26，代码完成，交互实机验证待做）

- 在 `keyboard_proc` 中对物理 F8 down 返回非零抑制；F8 up 一并抑制，避免目标应用看到孤立释放（NFR-04）。其余键与鼠标事件直通。
- 抑制的 F8 down 通过有界队列（容量 8）交给独立 replay worker；worker 延迟 300ms 后用 `SendInput` 一次注入 down+up 成对事件，并检查返回值（期望 2）。
- 注入事件写入本程序唯一 `dwExtraInfo` 标记 `0x494E_5055`（ASCII "INPU"）；回调据此识别自身事件并直通，从而注入不递归。
- `SendInput` 返回值不足（<2）时：记录失败、计数，并置旁路（停止拦截）；F12 作为紧急键切换旁路，本身永不拦截；控制台 `quit` 仍负责完整退出。
- 纯逻辑辅助（标记识别、F8/F12 键码）已加最小单测（`cargo test`，2 项通过）；Windows 特有行为按约定以实机验证为准。
- 已实机确认（本机）：`echo quit | probe-cli.exe` 能安装 Hook、打印 M2 横幅并以退出码 0 干净退出，无残留。

### 待实机验证（复现步骤，需人工按键）

- 记事本聚焦，按 F8：目标应只收到一次 F8（约 300ms 延迟）；日志应出现 `extra=0x494E5055 injected=true` 的注入 F8 down/up，且不再递归。
- 快速连按/长按 F8：无孤立释放、无卡键。
- 普通打字、Ctrl+Q 直通不受影响。
- F12 切换旁路往返：旁路后 F8 直通；再按 F12 恢复拦截。
- 提升权限窗口聚焦时按 F8：记录 `SendInput` 返回值与实际是否送达（UIPI 限制）。
- 每项记录：Windows 版本、规则、输入顺序、目标应用、预期/实际、是否丢键/卡键。

## 7. M3 核心状态模型（2026-09-26，纯引擎，Windows 实机 N/A）

- 仓库演进为 Cargo workspace（resolver 3）：`crates/inputflow-engine`（纯逻辑）+ `crates/inputflow-windows`（占位，M4 迁入平台代码）+ `apps/probe-cli`（成员，行为不变）；锁文件收敛到仓库根 `Cargo.lock`。
- `inputflow-engine` 零依赖、无任何 Windows 符号：`event`（`Key`/`MouseButton`/`MouseKind`/`InputSource`/`InputEvent`；`Key` 为平台无关枚举，`Unknown(u16)` 兜底不丢键）、`pending`（有界 FIFO 暂扣 + 溢出标志）、`state`（物理按住 / 目标已看到 / 已消费 三集合）、`matcher`（`Decision`/`Resolution`/`Command` + 可注入 `Clock`）。
- 匹配器把 M2 的 F8 暂扣泛化为 `Rule::Hold{key, timeout_ms, action}`，覆盖全部边界：候选 down 抑制、repeat 不重置计时、提前松开回放 `[down, up]`、超时命中消费（NFR-04：已消费 down 的 up 也被吞）、注入事件直通、溢出进入旁路并冲刷已有事件、暂停清理并停止新建暂扣。
- `on_event` 同步返回 `(Decision, Resolution)`；`on_timeout` 由 `next_deadline()` 驱动并显式推进时间，测试不依赖真实睡眠。
- 单测：`cargo test -p inputflow-engine` 18/18 通过；`cargo test --workspace` 20/20（含 probe-cli 回归 2 项）。`echo quit | target\debug\probe-cli.exe` 实机回归：Hook 安装、干净退出码 0。
- 待 M4 验证：`Key+Key`/`Key+MouseButton` 组合、`Resolution::Matched`（on_event 命中路径）、多候选竞争、VK→`Key` 映射的实机行为。

## 8. M4 组合匹配（2026-09-26，代码完成，组合交互实机验证待做）

- `inputflow-engine` 新增 `rules`（`Trigger::{KeyChord, KeyMouseButton, Hold}`、`Rule`、`RuleError`、`RuleIndex::compile` 预编译索引）；`matcher` 从单一 `Hold` 泛化为"首键暂扣 + 等待第二输入"的候选前缀状态机；`state` 新增 `MouseState` 跟踪鼠标按键物理/已见/已消费；`event` 补 `button()/is_button_down()/is_button_up()`。
- 组合语义（ADR-001）：首键 down 暂扣；第二输入完成规则 → 消费并 `Emit` 动作（仅一次）；首键松开或不匹配 → 按序回放 `[首键, 第二输入]`；组合无显式超时，重叠前缀 M4 不支持。`Matcher` 改为 `Send`（`ManualClock` 用 `Arc<AtomicU64>`，新增 `SystemClock`）。
- 平台代码自 `probe-cli` 迁入 `inputflow-windows`：`platform/windows`（Hook/消息循环/`SendInput`/注入标记/旁路）、`keymap`（VK↔Key、鼠标消息映射，纯逻辑可单测）。`Matcher` 在 Hook 回调内同步调用（`Mutex` 静态，短临界区），回放/动作经 `Command` 交给工作线程 `SendInput`。
- `probe-cli` 只做线程编排与规则定义；M4 演示规则 `LeftCtrl + RightButton → Ctrl+C`。
- 单测：`cargo test --workspace` 34/34（引擎 31 + keymap 3）；`cargo clippy --workspace` 无警告。`echo quit | target\debug\probe-cli.exe` 实机回归：Hook 安装、M4 横幅、退出码 0。

### 待实机验证（复现步骤，需人工按键）

- 记事本聚焦，按 Ctrl+Q（无对应规则）：目标应按序收到 Ctrl、Q，无丢键/卡键；日志应出现 `replay 2 held event(s)` 与 `output: 2 event(s) inserted`。
- 记事本聚焦，按住左 Ctrl 后点右键：应无原右键菜单弹出，`Ctrl+C` 动作触发一次；日志 `matched rule 'ctrl-right-click-copy'`；右键 up 与 Ctrl up 被消费、无孤立释放。
- 触发后继续按住左 Ctrl、快速连击、普通打字/快捷键直通、F12 旁路往返、提升权限窗口下的 `SendInput` 行为。

## 9. M5 时序规则（2026-09-26，代码完成，交互实机验证待做）

- `inputflow-engine` 新增 `Trigger::HoldMouseButton{key, timeout_ms, button}` 与 `RuleError::Conflict`；`RuleIndex` 增加 `hold_buttons`（每键至多一条）与 `has_hold_button`/`hold_button` 查询；`compile` 增加跨种类前缀冲突检测。
- `matcher` 新增 `Active::HoldToButton`（“等待阈值”/“已武装”两阶段）：`on_timeout` 在阈值到达且 `K` 仍按住时仅武装（不产出命令），此后 `B down` 才命中并消费；活动期间任意按钮 down 决定成败（MVP 不允许早于 `T` 的按钮）。新增 `poll_timeouts()` 供平台轮询。重复 down 不重置计时；`Hold` 阈值边界（精确 `T` 命中、`T` 前释放回放）补齐测试。
- 关键缺口修复：`on_timeout` 此前仅在单测中由 `ManualClock` 驱动；平台层新增消息循环线程周期 `SetTimer`（约 5ms，`WM_TIMER`）驱动 `poll_timeouts()`，使 `Hold`/`Hold+Button` 在 Windows 上真正生效。
- 冲突策略（ADR-002）：一个前缀键最多属于一种规则种类——单键 `Hold{K}` 与同前缀复合规则（含 `HoldMouseButton`）、`HoldMouseButton{K}` 与同前缀和弦、同键多条 `HoldMouseButton` 均拒绝启用。
- 单测：`cargo test --workspace` 47/47（引擎 44 + keymap 3）；`cargo clippy --workspace --all-targets` 无警告。`echo quit | target\debug\probe-cli.exe` 实机回归：Hook 安装、M5 横幅、退出码 0。

### 待实机验证（复现步骤，需人工按键）

- 记事本聚焦，按住左 Ctrl ≥250ms 后点右键：应无原右键菜单，`Ctrl+C` 触发一次；日志 `matched rule 'hold-ctrl-right-click-copy'`；右键 up 与 Ctrl up 被消费、无孤立释放。
- 记事本聚焦，按住左 Ctrl <250ms 即点右键（或提前松开）：应回放 `[Ctrl, 右键]` / `[Ctrl down, Ctrl up]`，无丢键/卡键。
- 按住左 Ctrl 期间自动重复 down 不重置计时；触发后继续按住、快速连击、普通打字/快捷键直通、F12 旁路往返、提升权限窗口下 `SendInput` 行为。

## 10. M6 可靠性（2026-09-26，代码完成，交互实机验证待做）

- 新增 `crates/inputflow-config`（serde + serde_json）：`Config{schema_version:1, emergency_bypass_key, rules}`；`load` 任何失败回退默认（空规则、F12）并记录问题（NFR-03）；`save` 写 `*.tmp`→flush/sync→rename（Windows 覆盖失败时 remove+rename 兜底）；默认路径 `%LOCALAPPDATA%\InputFlow\config.json`，`--config` 覆盖。
- 校验：类型、`timeout_ms∈[1,60000]`、重复 id、未知键名、保留组合（紧急键≠任意规则前缀键）、规则冲突（复用 `RuleIndex::compile`）；只把完整有效规则集交给引擎。
- 暂停/旁路（修复 ADR-002 缺口）：`suspend()` 先 `set_paused(true)` 取出并回放已暂扣事件再置 `BYPASS`；`resume()` 清 `BYPASS` 与 matcher overflow bypass；干净退出前 `flush_held()`。紧急键可配置（默认 F12）。
- 诊断日志：`replay`/`matched`/`timeout`/`queue_overflow`/`sendinput_failed`/hook 状态常开、匿名化；逐键事件移到 `--debug`（NFR-05）。
- 异常恢复：`%LOCALAPPDATA%\InputFlow\running` 启动写、干净退出删；下次启动残留则提示「上次异常终止，已暂扣输入不承诺恢复」。
- 性能采样：engine 新增 `stats::PercentileTracker`（有界滑动窗口 + nearest-rank 分位）；平台 `Instant` 采样「回调（matcher 决策）耗时」与「暂扣总延迟（首次 suppress→解析）」；`stats`/退出时报告 p50/p95/p99。
- 单测：`cargo test --workspace` 60/60（引擎 48 + 配置 9 + keymap 3）；`cargo clippy --workspace --all-targets` 无警告。
- 实机（本机，exit code 0）：缺失/损坏配置回退空规则旁路；`--config` 加载 1 条演示规则；`pause`/`resume`/`stats`/`quit` 生效；崩溃标记残留时启动告警、干净退出删除；`--print-default-config` 输出模板；`echo quit | probe-cli.exe` 干净退出。

### 性能基线（2026-09-26，本机 Windows 11 build 26200）

- 自动回归只覆盖「安装 Hook + 干净退出」，未产生输入事件，故回调耗时与暂扣延迟样本为 0（`total=0`，分位 `-`）。基线需在人工按键实机（记事本打字、Ctrl+Q 失败回放、Hold+右键命中）中采集样本后补记。

### 待实机验证（复现步骤，需人工按键）

- 记事本聚焦，按 F12（紧急键）：应暂停并冲刷已暂扣输入，日志 `suspended...`；再按 F12 恢复。
- 记事本聚焦，按住左 Ctrl ≥250ms 后点右键：`matched rule 'hold-ctrl-right-click-copy'`，无原右键菜单，`Ctrl+C` 一次；右键/Ctrl up 被消费、无孤立释放。
- 记事本聚焦，按住左 Ctrl <250ms 即松开或点右键：按序回放，无丢键/卡键。
- `pause` 后普通打字直通、`resume` 恢复拦截；逐键日志仅在 `--debug` 下出现，默认日志不含键名。
- 强杀进程（任务管理器）后重启：启动提示上次异常终止。
- 提升权限窗口聚焦时 `SendInput` 返回值（UIPI 限制）。

## 11. M6 第三轮可靠性加固（2026-09-27，自动化完成，交互/性能验收待做）

- **已证实并修复：消费释放配对**。原实现暂停时清空 consumed 集合，平台旁路又直接跳过 matcher；而 `DefWindowProc` 可由 `WM_RBUTTONUP` 生成 `WM_CONTEXTMENU`，所以“孤立 Up 总是无害”不成立。现在暂停、溢出和输出失败只清普通跟踪，保留已消费键/按钮的释放墓碑；旁路期间仍让事件经过 matcher，仅对应 Up/repeat 被抑制，其他输入直通。正常退出在卸载 Hook 前最多等待 2 秒排空墓碑；超时会记录 `shutdown_limit`，强杀同样无法保证拦截后续 Up。
- **已证实并修复：自动重复丢失**。活动前缀的 repeat Down 现在进入同一个有界 FIFO；KeyChord、KeyMouseButton、Hold、HoldMouseButton 的失败回放、命中消费、暂停冲刷与重复溢出均有事件序列测试。不再用不完整的按键类别拒绝 Hold 前缀。长时间重复填满 16 项平台队列时，当前重复直通、先前事件同步回放并进入旁路。
- **已证实并缓解：输出失败不等于恢复**。输出调用可注入，测试区分 0/部分/完整插入。返回 0 且当前事件是失败回放最后一项时，当前事件改为直通；更早的暂扣事件仍可能丢失。部分插入无法仅凭计数确定具体成功项，故不冒险重复发送；进入旁路、保留消费释放墓碑并打印 `last_error`。匹配动作部分执行、修饰键状态偏差及 UIPI 下旧输入无法恢复仍是明确残余风险。
- **已证实并修复：定时器 ready 顺序**。`SetTimer` 在 ready 前安装；返回 0 时报告错误、卸载 Hook 并令启动失败。请求间隔改为 Windows 实际最小值 10ms；`WM_TIMER` 低优先级，繁忙消息循环可使到期更晚，不能把 10ms 当作投递保证。
- **已证实并修复：配置提交/恢复**。写入使用 `create_new` 的 `config.json.tmp.<pid>.<seq>` 并 `flush+sync_all`；Windows 已有文件通过 `ReplaceFileW` 替换并保留唯一 `.bak.<pid>.<seq>`。正式文件缺失/损坏时，加载器从新到旧验证 `.tmp.*`/`.bak.*` 并载入首个完整有效副本。替换失败、首次提交失败、损坏正式文件恢复及四线程并发保存均有测试。仍不把它描述成断电条件下的无条件持久原子事务；跨进程同时写入可能有一次失败并留下可发现副本。
- **IF-06 仍待 Windows 交互实测**。采样范围已扩大为完整 Hook 回调墙钟时间（包括归一化、同步 `SendInput` 及转发路径的 `CallNextHookEx`）并增加 max。同步输出保持顺序，但 API 无最坏耗时承诺；本轮自动 smoke 只验证 Hook+10ms timer 安装/卸载和 exit code 0，输入样本仍为 0，不能据此宣称性能门槛通过。
- 自动验证：`cargo fmt --all -- --check` 通过；`cargo test --workspace` 为 85/85（engine 60 + config 15 + windows 10）；`cargo clippy --workspace --all-targets -- -D warnings` 通过。Windows smoke：缺失配置回退、Hook/定时器启动、`quit` 干净退出码 0；未发送人工键鼠输入。
- 详细证据、方案比较和剩余手工矩阵见 `check-fix-debug-list/tag_3_InputFlow-M6-三轮复查修复记录.md`。

## 变更记录

- 2026-09-26（M0）：建立四条主线的初始调研结论，均标注“待验证”；尚未进行 Windows 实机实验。
- 2026-09-26（M1）：完成只读探针 `apps/probe-cli`；实机验证 Hook 安装、事件捕获（injected/时间戳/顺序）与干净退出；修复 logger 持有 stdout 锁导致的死锁；确定 M2 抑制与回放方向。
- 2026-09-26（M2）：实现 F8 抑制 + 延迟回放（`SendInput`）+ 注入标记识别（`dwExtraInfo`）+ 紧急旁路（F12）；纯逻辑辅助已单测；已实机验证 Hook 安装/干净退出，交互按键行为待实机验证。
- 2026-09-26（M3）：演进为 Cargo workspace；新增纯逻辑 `inputflow-engine`（event/pending/state/matcher）并 18 项单测通过；`inputflow-windows` 占位；probe-cli 回归构建/退出通过。
- 2026-09-26（M4）：实现组合匹配（rules/matcher/state 扩展）、平台代码迁入 `inputflow-windows`（hooks/keymap/SendInput）、probe-cli 接入引擎；引擎 31 项 + keymap 3 项单测通过；`echo quit | probe-cli.exe` 实机回归通过；组合交互实机验证待做。
- 2026-09-26（M5）：实现时序规则（`Hold`/`HoldMouseButton`）、跨种类冲突检测、平台 `SetTimer` 驱动 `poll_timeouts`；引擎 44 项 + keymap 3 项单测通过；`echo quit | probe-cli.exe` 实机回归通过；交互实机验证待做。
- 2026-09-26（M6）：实现可靠性——新增 `inputflow-config`（版本化 JSON 配置、校验、原子保存、坏文件回退）、暂停/旁路（冲刷已暂扣）、可配置紧急键、匿名化诊断日志、崩溃标记、性能采样；引擎 48 + 配置 9 + keymap 3 共 60 项单测通过；`cargo clippy` 无警告；`echo quit | probe-cli.exe` 实机回归通过；交互实机验证待做。
- 2026-09-27（M6 三轮复查）：加入释放墓碑、完整重复事件暂扣/溢出语义、可注入输出结果、定时器启动门槛、`ReplaceFileW` 配置提交与恢复发现、完整回调墙钟采样；85 项测试与 Clippy 通过，Hook 启停 smoke 通过；真实键鼠/菜单/UIPI/高负载性能仍待人工验收。
