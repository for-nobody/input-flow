> [!CAUTION]
> 本文件是已废弃的 Tauri 时期历史记录，仅用于问题追溯。
> 不得将其中的前端架构、目录结构或 Tauri 命令作为当前实现要求。
> 当前任务以 `../tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md`
> 和 `../../docs/decisions/ADR-004-Rust常驻Agent与WinUI3设置程序.md` 为准。

# InputFlow M6 四轮后端修复记录

> 日期：2026-09-27  
> 基线：`893530b89d5e5ad1edfc7909ae56444aa04cef2e`（`main` / `origin/main`）  
> 范围：验证并处理 IF-07～IF-09，回归 IF-01～IF-06；不创建 Tauri/React 前端。  
> 证据等级：代码/确定性故障注入、Windows 无输入 lifecycle smoke、真实目标窗口观察严格分开。没有物理键鼠记录的项目均标为“未执行”。

## 1. 基线核对

- 开始时 HEAD 与任务基线一致；代码工作区干净，只有用户提供的 `tag_4_InputFlow-M6-四轮后端修复与Windows验收任务.md` 未跟踪。
- 仓库内没有 `AGENTS.md`。
- 已先阅读第三轮记录并保留 IF-01 的释放墓碑、IF-02 的 repeat 有界 FIFO、IF-03 的 0/部分输出诊断、IF-04 的 timer ready 门槛及 IF-05 的 `ReplaceFileW` 提交机制。

## 2. IF-07：跨线程暂停回放可被新物理输入超车

### 失败顺序与根因

基线顺序可由代码直接证明：

1. Hook 暂扣 `LeftCtrl↓`。
2. 控制线程取得 matcher 锁，`set_paused(true)` 取走 pending 并把 matcher 设为 paused，然后解锁。
3. 控制线程尚未进入 `dispatch_command` 时，Hook 线程取得 matcher 锁；新 `A↓` 因 paused 而 `PassThrough`。
4. 控制线程才调用 `SendInput(LeftCtrl↓)`，目标可观察到 `A↓ → LeftCtrl↓`。

`legacy_unlock_before_replay_model_proves_overtake` 用两个屏障稳定重建该交错，记录顺序为 `[new A(seq=1), replay Ctrl(seq=0)]`。这不是对真实桌面调度概率的估计，而是证明基线存在允许该结果的并发路径。

### 方案比较

1. **跨线程事务锁持有至 `SendInput` 返回**：能形成屏障，但 Hook 回调可能等待控制线程及无最坏耗时承诺的 `SendInput`；这会把顺序修复转化为 Hook 超时/静默移除风险，未选。
2. **只延后 BYPASS、加序号或无屏障 ack**：不能阻止 matcher 解锁后的 Hook 决策，不能证明目标顺序，未选。
3. **把 pause/resume 调度到 Hook 消息线程并在完成后确认**：Hook 回调、timer、F12、pause/resume 和退出冲刷由一个线程形成总序；控制线程不会持有 matcher 锁执行输出，选用。

### 实现语义

- 外部 `suspend()` / `resume()` 把带唯一 id 和单次回复通道的请求放入有锁控制队列，再以私有 `WM_APP+1` 线程消息唤醒 Hook 线程。
- Hook 线程执行 `set_paused(true) → 同步回放 → BYPASS=true → ack`。暂停 ack 包含 `held_events`、`requested_inputs`、`inserted_inputs`、`output_complete`、`last_error`；调用返回时旧 pending 已有可核查的交付结果。
- F12 已在 Hook 回调中，检测到当前线程就是 Hook owner 时直接执行，不向自己发消息/等待。外部 pause、F12 和 resume 无论交错如何都只能按 Hook 线程实际处理顺序生效。
- 外部等待上限为 2 秒：尚未开始的请求以原子状态取消；已经开始但未完成时返回“可能异步完成”的明确错误，不伪造完成状态。Hook 回调本身不等待控制线程。
- 请求唤醒失败会从队列撤回；若另一个唤醒已取走请求，则以真实 ack 为准。Hook 退出会拒绝并清空所有尚未处理请求。
- 正常退出继续在 Hook 线程直接冲刷；pending repeat 由既有 matcher FIFO 返回；SendInput 0/部分计数进入同一 `PauseReport`；释放墓碑仍由 matcher 在 paused/bypassed 检查之前处理。

### 修复后证据

- `pause_replay_cannot_be_overtaken_on_the_serial_hook_owner`：注入输出在屏障处停住后排入 `A↓`；在释放输出屏障前，输入处理完成通道必须超时，观察序列只有 Ctrl；释放后为 `[Ctrl(seq=0), A(seq=1)]`。
- 既有 `pause_flush_uses_the_same_incomplete_output_reporting`、0/部分/完整输出、repeat 暂停/溢出、墓碑暂停/恢复和关闭路径测试全部继续通过。
- Windows lifecycle smoke 实际完成 `pause → resume → stats → quit`，pause ack 为 `held_events=0 inserted=0 requested=0 output_complete=true last_error=0`，退出码 0。

## 3. IF-08：统计查询持锁排序与计时口径

### 根因与原行为

- 基线的 `callback_latency_stats()` / `hold_delay_stats()` 持 recorder mutex，依次调用 p50/p95/p99/max。
- p50/p95/p99 各复制并排序最多 100,000 个 `u64`，所以查询在锁内执行三次 `O(n log n)` 排序。Hook 的记录路径需要同一把锁。
- `finish_callback()` 原先先计算 `start.elapsed()`，再等待 recorder mutex；该等待延长 Hook 返回时间，却没有进入所记录的样本。
- 这是确定的竞争路径；本轮没有把它夸大为已经观察到 Hook 超时或静默移除。

### 方案比较与选择

1. **锁内复制一次、锁外排序一次**：精确保留当前最近 100,000 个样本和 nearest-rank 定义；查询瞬时额外内存上限约 800 KB/tracker；记录侧最多等待一次有界向量复制，而不是完整排序。选用。
2. **每次记录预计算/分片直方图**：查询快，但需改变精度模型、分桶边界或增加复杂合并状态；当前没有真实基线证明值得承担，暂不选。
3. **记录侧 `try_lock` 并丢样本**：Hook 最快，但会产生偏差且必须增加 dropped-sample 诊断；当前选项已移除主要排序竞争，不选。

### 实现与计时口径

- `PercentileTracker::snapshot()` 在锁内复制 `total + samples`；`PercentileSnapshot::summary()` 在锁外排序一次，同时计算 p50/p95/p99/max。
- `stats_sort_runs_after_releasing_the_recorder_lock` 填入 100,000 个非空样本，在 summary 屏障处停止查询；另一线程仍须在 1 秒内记录第 100,001 个样本。查询快照 total 保持 100,000，tracker total 成为 100,001，证明记录没有等待完整排序。
- 固定样本测试验证环形窗口覆盖后的 total/retained 和 nearest-rank 结果。
- callback 指标更名为 **observed duration**：从回调入口计到成功取得 recorder mutex 后读取 elapsed。它包含归一化、held-key/matcher/recorder 锁等待、同步 `SendInput`、非阻塞日志入队，以及直通路径 `CallNextHookEx`；不包含随后发生的样本写入、mutex 解锁和函数最终返回，故不再称为无法自测尾部的“完整 Hook wall time”。
- hold delay 从第一次 pending suppress 计至解析并取得 hold-delay recorder 锁，发生在实际 `SendInput` 之前，因此不包含输出调用。
- CLI 的 `stats query duration` 只包住两个快照+汇总调用，不包含随后控制台打印时间。

## 4. IF-09：恢复优先级与容量策略

### 判断

- 当前正式配置没有已证实丢失；问题是未来自动保存会令唯一命名 backup 无界增长，以及“最新 mtime 的 temp”可能压过曾正式提交的 backup。
- `.bak` 是 `ReplaceFileW` 在成功替换时留下的上一份正式配置；`.tmp` 是未成功提交的尝试。因此“用户最后确认启用”的默认恢复语义应先选有效 backup，再把 temp 作为没有有效 backup 时的最后救援。

### 策略与实现

- 正式文件有效时始终直接使用正式文件。
- 正式文件缺失/无效时：全部 backup 按 mtime 新到旧验证；没有有效 backup 才验证 temp。诊断明确写“committed backup”或“uncommitted temporary save”。
- backup 保留 5 代，temp 保留 3 份。当前 JSON 通常很小，容量约为 `5 × 旧正式配置 + 3 × 未提交尝试`，不再随正常 autosave 次数线性增长。
- 清理先验证同类副本：保留最新 N 份；若它们全损坏，额外保护限额外最新的有效副本。因此不会仅为满足数量上限而删除唯一已知有效恢复副本。
- 成功提交后清 backup/temp；提交失败后清 temp，并把删除失败附加到保存错误。成功提交后的清理是 best-effort，因为现有 `save() -> Result<(), String>` 无 warning 通道；正式 UI 接线前应升级为可携带 warning 的保存报告。
- 进程在写入与清理之间被强杀仍可能每次留下一份额外 temp；下一次能走到保存完成/失败处理时会再次收敛。

### 测试

- `committed_backup_is_preferred_over_newer_uncommitted_temp`：损坏 primary，同时放置 F1 backup 和更新的 F2 temp，恢复 F1 并标记 committed backup。
- `successful_saves_bound_committed_backup_generations`：连续保存 10 个有效版本后恰有 5 个 backup。
- `retention_never_deletes_the_only_valid_artifact`：3 个限额内损坏 temp 之外的唯一有效旧 temp 被额外保留。
- 第三轮替换失败、首次提交失败、并发保存与损坏 primary 恢复测试继续通过。

## 5. 自动验证原始结果

环境：Windows 11 Pro 10.0.26200（AMD64），Rust stable 1.98.1，`x86_64-pc-windows-msvc`。

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| `cargo test --workspace` | 92/92：engine 61、config 18、windows/keymap 13、probe-cli 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| `cargo build -p probe-cli` | 通过 |
| `pause → resume → stats → quit` lifecycle smoke | Hook/timer/control ack/退出通过，exit code 0；无物理输入，callback/hold 样本均为 0 |

Cargo 每次打印 `warn: could not canonicalize path C:\Users\for-nobody`，但命令返回 0；它未造成构建、测试或运行失败。smoke 启动时还观察到既有 `%LOCALAPPDATA%\InputFlow\running` 异常会话警告；该外部标记不属于仓库文件，本轮不把警告当成输入行为结果。

## 6. Windows 真实输入验收记录与人工步骤

本会话能运行 Windows 程序，但不能生成可信的**物理**键鼠，也没有可读取 `WM_KEYDOWN/UP`、鼠标消息和 `WM_CONTEXTMENU` 的外部目标窗口。因此以下真实输入项均为 **未执行**；单测/smoke 不替代它们。

### 统一准备

1. 建立 `target\tag4-manual`，打开 PowerShell，执行 `Start-Transcript -Path target\tag4-manual\probe.log -Force`。
2. 准备所测规则配置并运行 `cargo run -p probe-cli -- --config <配置路径> --debug`。
3. 同时打开普通目标（记事本）和消息记录窗口；后者必须保存 `WM_KEYDOWN/WM_KEYUP`、鼠标 down/up 与 `WM_CONTEXTMENU` 的时间顺序到 `target\tag4-manual\target-messages.log`。若用 Spy++ 或自有测试窗口，记录工具名/版本。
4. 每个场景保存：系统版本、键盘布局、目标进程及权限、配置、完整手势、probe 日志区间、目标消息区间、可见结果和是否仍能继续捕获。
5. 完成后输入 `quit`，执行 `Stop-Transcript`。不要用强杀结束正常场景。

### 待回填矩阵

| 项目 | 状态 | 手势/步骤 | 预期 | 必记观察 |
| --- | --- | --- | --- | --- |
| 墓碑 + F12 | 未执行 | 命中 `Hold(LeftCtrl)+Right` 并保持按键；按 F12；释放 Right/Ctrl | 目标无孤立 Right Up 触发的菜单；普通新输入直通 | `WM_RBUTTONUP/WM_CONTEXTMENU`、BYPASS、释放顺序 |
| 墓碑 + 控制台 pause/resume | 未执行 | 同上，另测 `pause` 后立即 `resume`、KeyMouseButton 与左键 | pause ack 在旧 pending 交付后；resume 不丢墓碑 | pause report 五字段、目标消息顺序 |
| repeat 四分支 | 未执行 | KeyChord(A,B)、KeyMouseButton(A,Right)、Hold(F8)、HoldMouseButton；长按 A/F8 后分别失败、命中、pause、填满 16 项 | 字符/功能键重复数与回放 FIFO 一致；溢出后当前项直通并旁路 | repeat 数、replay 数、目标事件逐项对照 |
| IF-07 目标顺序 | 未执行 | 暂扣 Ctrl 后在另一个人/脚本输入 A 的同时从控制台 pause；重复至少 100 次 | 不出现 `A↓ → replay Ctrl↓`；pause 返回前 replay 已有结果 | 两端时间戳、pause report、任何逆序样本 |
| SendInput 权限 | 未执行 | 对普通和提升权限目标分别触发失败回放与动作 | 普通目标完整；提升目标可能 0（UIPI），进入旁路且不虚称历史恢复 | inserted/requested/last_error、目标消息、墓碑 Up |
| 部分插入 | 未执行（实机触发不稳定） | 若无法稳定构造，只保留故障注入测试 | 不重复发送无法定位的成功项；进入旁路 | 明确写“未实机触发”，不得记通过 |
| stats 高负载 | 未执行 | 连续打字/点击 5 分钟；另一人每秒输入 `stats` 或 UI 等价轮询 | Hook 持续捕获；查询不造成可见卡键/丢键 | 每次 query us、total/p50/p95/p99/max、开始/结束 seq |
| 左右/扩展/布局 | 未执行 | 左右 Ctrl/Alt/Shift、扩展键；至少 US 与实际常用布局；移动鼠标后触发回放；预先按住修饰键再触发 action | 结果符合当前 VK+extended 模型；差异被记录为边界 | 布局、scan/vk、光标位置、动作实际修饰状态 |
| 正常退出 ≤2s | 未执行 | 命中并保持 consumed 键/右键，输入 quit 后 2 秒内释放 | drain 收到 Up 后干净退出，无 orphan Up | `shutdown_drain`、退出耗时、目标消息 |
| 正常退出 >2s | 未执行 | 同上但超过 2 秒才释放 | 约 2 秒后 `shutdown_limit` 并卸 Hook；明确后续 orphan Up 风险 | 实际等待、日志、目标是否出现菜单/孤立 Up |

高负载暂定验收阈值：连续 5 分钟无 Hook 捕获中断、无可复现丢键/卡键；每次 stats 数据查询建议 `< 20 ms`，callback observed p99 建议 `< 5 ms`、max `< 100 ms`。这些是保守的首轮筛查值，不是已建立的产品 SLO；若失败，保留原始样本并按目标硬件/共存软件重新分析，不能调高阈值后直接宣称通过。

## 7. 交付表

| 编号 | 根因及证据 | 方案取舍 | 修改文件/关键位置 | 自动测试 | Windows 实测 | 剩余限制 |
| --- | --- | --- | --- | --- | --- | --- |
| IF-07 | matcher 先 paused/解锁，控制线程后回放；屏障模型得到 `[A, Ctrl]` | 选 Hook 线程控制队列 + 完成 ack；拒绝跨线程持锁输出和无屏障序号 | `inputflow-windows/platform/windows.rs`、`probe-cli/main.rs` | legacy 失败模型、串行 owner 屏障、既有输出/墓碑/repeat 回归 | lifecycle 通过；物理目标未执行 | 同步 `SendInput` 本身仍无最坏耗时；2 秒 ack 后已开始操作可能晚完成 |
| IF-08 | 查询锁内三次排序；elapsed 在 recorder 锁等待前读取 | 选锁内一次快照、锁外一次排序；不丢样本 | `engine/stats.rs`、`windows.rs`、`probe-cli/main.rs` | 固定分位 + 100k 非空并发屏障通过 | 无输入 stats smoke 通过；高负载未执行 | 锁内仍复制最多约 800KB；真实 Hook 存活/分位未测 |
| IF-09 | backup 无界；新 temp 可压过已提交 backup | backup 优先；5/3 代有效性保护保留 | `inputflow-config/config.rs` | 优先级、10 次保存上限、唯一有效保护、既有故障注入通过 | 文件系统测试在 Windows 通过 | 强杀可留下额外 temp；成功保存的 cleanup warning 暂无返回通道 |
| IF-01～IF-06 回归 | 第三轮语义保留，全部既有测试通过 | 不撤销墓碑/FIFO/timer/输出诊断/ReplaceFile | engine/config/windows 全部相关模块 | workspace 92/92，Clippy/fmt/build 通过 | Hook/timer/control lifecycle 通过；真实键鼠/UIPI/高负载未执行 | IF-03 历史暂扣恢复、IF-06 最坏输出耗时仍无真实保证 |

## 8. 完成门槛结论

- **可以讨论 UI 设计稿**：可以；与 Hook 正确性解耦。
- **可以开始不启动 Hook 的静态 UI 骨架**：可以；不得暗示规则已在系统中生效。
- **可以把规则启用、暂停、配置保存接入真正的 Tauri 命令**：尚不建议。IF-07/08 的后端接口门槛已处理，但应先完成上表真实 pause 顺序、高负载和保存 warning API 的人工/接口验收。
- **可以作为稳定输入工具持续运行**：不可以宣称。真实菜单、repeat、UIPI、布局/扩展键、鼠标位置、退出 2 秒边界及高负载 Hook 存活均未执行。
