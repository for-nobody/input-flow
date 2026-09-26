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

## 变更记录

- 2026-09-26（M0）：建立四条主线的初始调研结论，均标注“待验证”；尚未进行 Windows 实机实验。
- 2026-09-26（M1）：完成只读探针 `apps/probe-cli`；实机验证 Hook 安装、事件捕获（injected/时间戳/顺序）与干净退出；修复 logger 持有 stdout 锁导致的死锁；确定 M2 抑制与回放方向。
- 2026-09-26（M2）：实现 F8 抑制 + 延迟回放（`SendInput`）+ 注入标记识别（`dwExtraInfo`）+ 紧急旁路（F12）；纯逻辑辅助已单测；已实机验证 Hook 安装/干净退出，交互按键行为待实机验证。
- 2026-09-26（M3）：演进为 Cargo workspace；新增纯逻辑 `inputflow-engine`（event/pending/state/matcher）并 18 项单测通过；`inputflow-windows` 占位；probe-cli 回归构建/退出通过。
