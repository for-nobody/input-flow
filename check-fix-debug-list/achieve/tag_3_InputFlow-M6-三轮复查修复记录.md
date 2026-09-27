> [!CAUTION]
> 本文件是已废弃的 Tauri 时期历史记录，仅用于问题追溯。
> 不得将其中的前端架构、目录结构或 Tauri 命令作为当前实现要求。
> 当前任务以 `../tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md`
> 和 `../../docs/decisions/ADR-004-Rust常驻Agent与WinUI3设置程序.md` 为准。

# InputFlow M6 三轮复查修复记录

> 日期：2026-09-27  
> 基线：`a7aff2dabd0e6666318f43afd7fa73fc95459eb4`（`main` / `origin/main`）  
> 范围：验证并处理 `tag_3_InputFlow-M6-三轮复查与Codex系统修复任务.md` 的 IF-01～IF-06；不实现 Tauri/React UI。  
> 证据约定：本文把“代码/确定性测试可证”“Windows Hook 启停 smoke”“真实目标窗口人工观察”分开。未执行的人工场景一律标为未验证。

## 1. 基线与项目结论

- 开始时工作区只有用户提供的 `tag_3` 清单未跟踪，代码无未提交改动；HEAD 与清单一致。
- 原始基线：`cargo test --workspace` 为 67/67；`cargo clippy --workspace --all-targets -- -D warnings` 通过；`cargo fmt --all -- --check` 失败，原因是仓库中已有 Rust 格式差异，不是业务测试失败。
- 已建成：纯 Rust 规则引擎（KeyChord、KeyMouseButton、Hold、HoldMouseButton）、Win32 低级键鼠 Hook/同步 `SendInput`、版本化 JSON 配置、暂停/紧急键、诊断、崩溃标记与分位采样。
- 原计划下一阶段是 M7（Tauri 2 + React + TypeScript 规则编辑器）。本轮结论是：视觉/工程骨架可独立推进，但允许用户创建并启用真实规则的 UI 接线尚未达到门槛，因为 IF-06 和若干输入身份边界仍缺真实窗口/高负载证据。

## 2. 逐项结论

| 编号 | 清单结论是否成立 | 本轮状态 | 证据等级 |
| --- | --- | --- | --- |
| IF-01 | 成立；“孤立 Up 无害”被 Win32 右键语义直接反驳 | 已修复暂停/旁路期间的代码路径；退出/强杀边界保留并诊断 | matcher 序列测试 + 官方语义；真实菜单未验证 |
| IF-02 | 成立；四类规则共用的 repeat 分支会抑制但不保存 | 已修复；所有 repeat 都进入有界 FIFO，不再用不完整白名单拒绝 | 纯 matcher/config 测试 |
| IF-03 | 成立；`BYPASS` 只能保护之后的输入 | 已缓解并准确诊断；0/部分/完整可注入，零插入时有限恢复当前事件 | 平台故障注入；UIPI 真实失败未验证 |
| IF-04 | 成立；ready 先于 `SetTimer`，失败后仍运行 | 已修复；timer 成功成为 ready 前置条件 | 注入返回 0 测试 + Windows 启停 smoke |
| IF-05 | 成立；双 rename 有正式路径空窗且恢复错误被忽略 | 已修复提交协议与启动恢复发现 | 文件故障注入 + 并发测试；断电未验证 |
| IF-06 | 是风险，不是已实测缺陷 | 采样口径已补全并加 max；同步输出最坏耗时仍未验收 | Windows 真实输入样本为 0，未通过门槛 |

## 3. IF-01：已消费 Down 的释放墓碑

### 复现、预期、原实际

事件序列：`LeftCtrl↓`（暂扣）→ 阈值到期 → `Right↓`（命中并消费 Ctrl/Right Down）→ `pause/F12` → `Right↑` → `LeftCtrl↑`。

- 预期：不复活已消费的 Down；暂停后普通输入立即直通；Right/Ctrl 的对应物理 Up 仍被消费。
- 原实际：`set_paused(true)` 清空 `consumed`，平台在 `BYPASS` 时绕过 matcher，两个 Up 直接进入下游。
- 证据：Microsoft 文档明确说明 `DefWindowProc` 处理 `WM_RBUTTONUP` 时会生成 `WM_CONTEXTMENU`，所以原注释“应用忽略未见 Down 的 Up”不能成立。是否在某个具体窗口弹出菜单仍需真实窗口验证。

### 方案比较与实现

- 放弃“暂停时合成 consumed Down”：它会复活本应消费的点击/按键，且鼠标可能已移动。
- 选用“释放墓碑”：pending 事件仍冲刷；已命中且 Down 被消费的键/按钮只保留 consumed 身份，直到对应物理 Up。旁路继续让事件经过 matcher，但除墓碑对应 repeat/Up 外全部直通。
- `crates/inputflow-engine/src/matcher.rs`：在 paused/bypassed 判断之前处理 consumed；`set_paused`/overflow 保留墓碑，并暴露 `has_release_tombstones`。
- `crates/inputflow-engine/src/state.rs`：新增只清普通 tracking、不清 consumed 的转换操作。
- `crates/inputflow-windows/src/platform/windows.rs`：移除 BYPASS 的直接 `CallNextHookEx` 快路径；输出失败也同步设置 matcher pause/bypass。正常退出时继续泵消息最多 2 秒以排空墓碑，超时记录 `shutdown_limit` 后才卸载 Hook。

### 自动验证与限制

- `pause_keeps_consumed_releases_as_tombstones`：暂停时普通 A 直通，Right Up/Ctrl Up 抑制。
- `resume_does_not_discard_consumed_release_tombstones`：暂停后立即恢复也不能丢墓碑。
- 未验证：真实 `WM_CONTEXTMENU` 窗口、KeyMouseButton/左键变体、控制台 pause 与 F12 交错。
- 固有限制：正常退出的排空等待有 2 秒上限，防止设备消失/缺失 Up 导致永久挂起；超时后卸载 Hook，之后的 Up 无法继续抑制。强杀没有排空机会。

## 4. IF-02：前缀自动重复不再静默丢失

### 复现、根因与语义

最小序列：KeyChord(A,B) 下 `A↓, A repeat↓ × N, X↓`。原 `on_first_key_event` 对 repeat 返回 `Suppress/Pending` 却不入队，因此失败只回放初始 A 与 X。相同分支服务四类规则；配置却只对 Hold/HoldMouseButton 的部分键做分类拒绝，且 F1～F24 与测试模型矛盾。

选定语义：

- 活动前缀的每个 repeat Down 都进入有界 pending FIFO；重复不重置 Hold 计时。
- 失败或暂停：按原序回放初始 Down、全部已保存 repeat 和决定性事件。
- 命中：初始 Down 与 repeat 一起消费。
- 队列满：同步回放已保存事件，当前 repeat 直通，进入旁路；不会静默丢弃，也不会无限增长。

因此删除了 `Key::auto_repeats` 分类和 Hold 专用拒绝；KeyChord、KeyMouseButton、Hold、HoldMouseButton 对字母、数字、功能键、修饰键采用同一可测试语义。

### 自动验证

- 四类失败：`key_chord_failure_replays_prefix_repeats_in_order`、`key_mouse_failure_replays_prefix_repeats_in_order`、`hold_failure_replays_prefix_repeats_in_order`、`hold_mouse_failure_replays_prefix_repeats_in_order`。
- 命中/暂停/溢出：`successful_match_consumes_retained_prefix_repeats`、`pause_replays_retained_prefix_repeats`、`repeat_overflow_replays_buffer_and_passes_current_repeat`。
- 配置：`every_trigger_kind_accepts_a_repeating_prefix`。
- 未验证：Windows 对 A、数字、F8、修饰键实际产生 repeat 的矩阵，以及文本框中字符次数/顺序。

## 5. IF-03：输出故障注入与有限 fail-open

### 原路径与不能承诺的边界

原代码在 `SendInput` 返回不足时只置平台 `BYPASS`，但 Hook 仍按匹配器原决定抑制当前事件；先前 pending 已被取走。清单判断成立。官方 API 只返回成功插入数，UIPI 原因不能由返回值或 `GetLastError` 精确识别；“进入 BYPASS”绝不等于“已恢复先前输入”。

### 实现

- 输出构建与发送之间加入可注入闭包，产生 `Complete / ZeroInserted / PartiallyInserted` 报告，记录 requested、inserted 与立即取得的 last_error。
- 完整插入：维持原决定。
- 零插入 + `Failed { replay }` 且当前事件为 replay 最后一项：当前 Hook 决定改为 PassThrough；更早的 pending 仍可能丢失。
- 部分插入：仅凭总数不能安全断言具体哪个事件已到达，故不重复发送/不放行可能重复的当前事件；进入旁路并明确记录残余风险。
- Matched action 失败：不复活已消费触发输入，保留释放墓碑。动作可能只执行一部分，特别是修饰键状态仍可能偏离；当前版本无法严格恢复。
- 输出失败、timer 动作失败和之后的事件都进入 matcher pause/bypass；后续普通输入放行，墓碑 Up 仍被消费。

### 自动验证与未验证项

- `output_fault_injection_distinguishes_zero_partial_and_complete` 检查实际构建数组长度和三种计数。
- `zero_replay_forwards_only_the_current_event` 检查零插入/部分插入的 Hook 决策差异。
- `failed_action_never_reanimates_consumed_trigger_input` 检查 Hold/匹配动作零插入时不复活触发。
- 暂停冲刷复用同一 `Command::Replay` 输出路径；engine 已验证 pause 返回的完整 FIFO。
- 未验证：普通权限→提升权限窗口的 UIPI、真实部分插入、目标程序最终收到的消息。`SendInput` 成功计数也只证明插入输入流，不证明目标程序完成动作。

## 6. IF-04：timer 成为启动前置条件

原顺序是 Hook → ready → `SetTimer`；返回 0 仅记录警告。现顺序是 Hook → 建消息队列 → 安装 timer → ready。timer 失败会把错误发给主线程、卸载两个 Hook 并退出，不会启用一个无法到期的 Hold 引擎。

- `install_timeout_timer_with` 是故障注入点；`timer_install_failure_is_fatal_before_ready` 强制 `(0, error)` 并验证 fatal 结果。
- `SetTimer(NULL, 0, 10ms, NULL)` 使用系统生成 ID；清理使用实际返回 ID。
- 原“约 5ms”表述不正确：小于 `USER_TIMER_MINIMUM` 会被钳到 10ms。即使 10ms，`WM_TIMER` 也是低优先级消息，繁忙队列会延迟；matcher 用单调时钟的 `now >= deadline` 语义在晚 tick 到来时解析，但动作同样会晚。
- Windows smoke 已证明本机 timer 安装成功并能进入/退出消息循环；真实繁忙队列延迟未测。

## 7. IF-05：配置替换、恢复与并发

### 原因与方案

原 `target → backup`、`tmp → target` 的双 rename 在两步之间没有正式文件，崩溃后 `load` 又不查 backup；最终 rename 失败时恢复结果被忽略；PID-only 名字会被同进程并发保存复用。清单判断成立。

选定 Windows 协议：

1. 校验整个配置；无效配置拒绝保存。
2. 以 `create_new` 创建 `config.json.tmp.<pid>.<seq>`，写入后 `flush + sync_all`。
3. 首次创建用同目录 rename；已有正式文件用 `ReplaceFileW(target, tmp, unique_backup, 0, ...)`，不先删除/搬走 target。
4. 失败不删除有效 tmp；`load` 在正式文件缺失、不可读或无效时，从新到旧验证 `.tmp.*`/`.bak.*`，载入第一个完整有效副本并给出“需再次保存使其正式化”的诊断。
5. 进程内保存由 mutex 排序，原子序号保证名字不复用。跨进程竞争仍可能使其中一次提交失败，但不会覆盖对方的恢复副本。

### 自动验证与限制

- 正常首次/覆盖 round-trip；覆盖后保留一个有效旧配置 backup。
- `failed_replace_keeps_official_config_and_recovery_temp`：注入替换失败，正式旧配置不变，新 tmp 可发现。
- `failed_first_commit_is_recovered_from_valid_temp`：首次提交失败且正式路径不存在，重启加载新 tmp。
- `corrupt_official_config_recovers_valid_backup`：损坏正式文件时使用有效 backup。
- `concurrent_saves_do_not_share_artifact_names`：四线程保存全部成功，正式配置完整，artifact 名不冲突且无未提交 tmp。
- 限制：没有模拟突然断电/文件系统缓存丢失；Windows 文档还列出 `ReplaceFileW` 的多种部分失败文件布局，因此 loader 以“扫描并完整验证”而不是假定单一布局恢复。成功覆盖会保留历史 backup，后续需为 GUI 制定保留/清理上限。

## 8. IF-06：性能风险的当前处理

清单把它标为风险而非确定缺陷是正确的。本轮没有证据支持“同步 `SendInput` 足够快”，也没有恢复异步 worker/无标识 ack/阻塞队列方案。

已改进：

- 采样从 `process()` 内部扩大到 Hook 回调入口至返回，包含事件归一化、debug 日志入队、matcher 锁、同步输出，以及 PassThrough 路径的 `CallNextHookEx`。
- `stats` 新增 max，并保留 p50/p95/p99/total。
- 文档删除“SendInput 有界耗时”的无依据断言，明确同步输出是顺序正确性与 Hook 超时风险之间的取舍。

仍未解决：

- `SendInput` 仍在 matcher 锁持有期间同步调用；它保证回放先于随后直通事件，但没有 API 最坏耗时保证。
- 未设置“可接受阈值”，因为尚无真实基线；不能用故障注入闭包的即时返回代替 Win32 性能。
- 本轮 smoke 的 `callback wall time total=0`，没有输入样本，不能证明 Hook 高负载存活。

## 9. 最终验证

实际执行：

```text
cargo fmt --all -- --check
  PASS

cargo test --workspace
  PASS: inputflow-engine 60, inputflow-config 15,
        inputflow-windows 10, probe-cli 0; total 85

cargo clippy --workspace --all-targets -- -D warnings
  PASS: 0 warnings

cargo build -p probe-cli
"quit" | target\debug\probe-cli.exe --config target\tag3-smoke\missing-config.json
  PASS: Hook + timer ready, clean exit code 0
  callback wall time total=0; hold delay total=0
```

Smoke 启动时发现一个本轮开始前已存在的 `%LOCALAPPDATA%\InputFlow\running` 标记并打印异常终止警告；受执行环境文件权限约束，本轮未删除该外部标记。它不影响仓库测试结论。

## 10. 尚未执行的 Windows 人工矩阵

1. 能记录窗口消息的普通窗口：已消费 Right Down → F12/pause → Right Up，不出现 `WM_CONTEXTMENU`；再测 KeyMouseButton、左键、暂停后快速恢复。
2. 文本框：KeyChord(A,B)、KeyMouseButton(A,Right)、Hold(F8)、HoldMouseButton 的 repeat 失败/命中/暂停/队列溢出；对照字符次数与顺序。
3. 失败回放后立刻 Up/下一键：验证真实目标顺序，而不只看 matcher replay 数组。
4. 普通窗口与提升权限窗口：记录 `SendInput` 的 0/部分/完整计数、目标消息与旁路/墓碑状态。
5. 持续打字、快速鼠标、高负载、多规则：记录样本量、p50/p95/p99/max、Hook 是否仍捕获，并据此决定阈值与同步输出架构。
6. 左右 Ctrl/Alt/Shift、扩展键、不同布局：确认 VK 回放边界；移动鼠标后失败回放按钮，确认“当前位置”语义；已有其他修饰键按住时验证 action。
7. 正常退出时仍按住 consumed 输入：2 秒内释放应被墓碑消费；超过 2 秒应出现 `shutdown_limit` 并退出，验证不会永久挂起。

## 11. UI 集成门槛

IF-01～IF-05 的确定代码问题已做实质修复并有自动回归；IF-03/退出路径仍保留由 Win32 权限和进程生命周期决定的明确限制。IF-06 未有真实输入性能数据，低优先级的 VK/鼠标位置/物理修饰键边界也未完成实测。

因此本轮结论不是“可以开放正式规则 UI”。下一步应先执行第 10 节并回填观察日志；性能与 Hook 存活达到经记录的阈值后，再接 M7 的规则启用、暂停、IPC 和保存。视觉稿与不会启动 Hook 的前端骨架可以并行准备。

## 12. 官方依据

- `WM_CONTEXTMENU` / `DefWindowProc`：https://learn.microsoft.com/en-us/windows/win32/menurc/wm-contextmenu
- `SendInput` 返回数量、UIPI、序列化与现有按键状态：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- LowLevel Hook 超时/静默移除：https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
- `SetTimer` 失败、最小间隔与 `WM_TIMER`：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-settimer
- `ReplaceFileW` 备份与部分失败布局：https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew
