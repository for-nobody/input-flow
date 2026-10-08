> [!CAUTION]
> M6 历史任务，仅用于问题追溯。Tauri 架构、旧路径和任务状态均已失效；当前状态见
> [`../../status/CURRENT_STATUS.md`](../../status/CURRENT_STATUS.md)。

# InputFlow M6：下一轮后端修复与 Windows 验收任务

> 给 VS Code 中的 Codex 使用。当前任务只处理输入引擎及诊断的可靠性；前端 UI 的信息架构、视觉风格和交互稿将由用户另行讨论。**本任务不创建 Tauri/React 页面，不接入真实规则编辑 UI。**

## 1. 基线和已知事实

- 基线代码：input-flow(3).zip，Git HEAD 893530b（第三轮修复提交）。请在工作前核对当前 HEAD、未提交改动及是否存在 AGENTS.md；以你实际打开的仓库为准。
- 上轮修复记录：check-fix-debug-list/tag_3_InputFlow-M6-三轮复查修复记录.md。请先阅读，再检查关键实现，勿仅凭结论修改。
- 本轮静态复查确认：IF-01 的 consumed release tombstone、IF-02 的 repeat 入有界队列、IF-04 的 timer 初始化前置、IF-05 的配置替换/恢复机制均已落入代码；IF-03 的输出失败已能区分零/部分插入，但历史暂扣输入仍可能不可恢复。保留这些有益改动及既有回归测试。
- 上轮记录称：cargo test --workspace 共 85 项通过，cargo fmt --all -- --check 和 cargo clippy --workspace --all-targets -- -D warnings 通过；Windows 上只做了 Hook/timer 启停 smoke，callback wall time 样本为 0。复查环境没有 Cargo/Windows，**未独立重跑**；请在你的 Windows 工作区亲自执行并记录真实结果。
- 确定的新问题为 IF-07 的跨线程暂停顺序竞态；IF-08 为统计查询可阻塞 Hook 的确定代码路径，其实际耗时影响需要测量。IF-09 是正式 UI 保存功能到来之前的存储策略检查，不等同于当前配置已丢失。

## 2. 处理原则与流程

1. 每项先给出事件顺序或线程交错、预期行为、当前行为和原因假设。把“代码可证明”“故障注入可证明”“Windows 真实目标程序已观察”明确分开。
2. 先设计能稳定暴露缺陷的测试；再提出至少两种可行修法及其对 Hook 耗时、回放先后、锁、关闭流程、F12 紧急旁路的影响。选择一项后再改代码。
3. 不恢复旧的无标识 ack 或在 Hook 中进行无期限阻塞发送。任何方案都必须在规则失败、暂停、超时、溢出、退出和 SendInput 故障时说明输入顺序。
4. 完成时提供代码改动、失败前/修复后的测试证据、Windows 手工记录、没有验证的边界及剩余风险。不能用单元测试或一次无输入的启动 smoke 代替真实键鼠测试。

## 3. IF-07（必须修复）：控制台/未来 UI 线程暂停时，回放可能被新物理输入超车

### 代码证据

- crates/inputflow-windows/src/platform/windows.rs 第 162–173 行：flush_held_events() 在 Matcher 锁内调用 set_paused(true)，取走 pending；**释放锁后**才调用 dispatch_command() / SendInput 回放。
- 同文件第 183–186 行：suspend() 等回放之后才写入 BYPASS。Matcher 已处于 paused 时，Hook 的 process() 会把新输入作为 PassThrough。
- apps/probe-cli/src/main.rs 第 227–251 行：控制台 pause 在主线程调用 windows::suspend()；Hook 在另一个线程。未来 Tauri 命令线程也可能出现相同交错。
- Hook 自己处理 F12 时，两步目前在同一 Hook 线程；不要把该路径的顺序性质直接推广到控制台/未来 IPC。

### 最小交错（静态可证，实际调度需测试）

1. 规则 Hold(LeftCtrl,250ms)+RightButton 正在等待；物理 LeftCtrl Down 已暂扣，目标应用尚未见到。
2. 控制台线程执行 pause：set_paused(true) 取出 LeftCtrl Down，然后释放 Matcher 锁；测试屏障在此暂停控制台线程。
3. Hook 线程收到用户 A Down：Matcher 已 paused，于是 PassThrough；Hook 返回，A 可以先到目标应用。
4. 控制台线程继续：SendInput 回放先前的 LeftCtrl Down。目标观察顺序可能成为 A Down → Ctrl Down，而不是 Ctrl Down → A Down；例如 Ctrl+A 的行为被改变。

### 修复与验收

- 先写具有明确同步屏障的双线程回归测试或可注入的平台级序列测试。不能只测试 set_paused 返回的数组顺序，因为问题在解锁与真正 SendInput 之间。
- 在设计评审中比较：把暂停/冲刷操作调度到 Hook 所属线程；或为暂停、Hook 决策和同步输出建立严格的串行化边界；或其他能证明顺序的方案。只延迟设置 BYPASS、只锁住 set_paused、给输出命令加序号但不做屏障，都不足以自动证明目标顺序。
- 若选择跨线程持锁直到 SendInput 返回，需评估 Hook 等锁时间以及系统静默移除 Hook 的风险，并进行故障注入/压力验证。不得修好顺序却引入长时间 Hook 阻塞。
- 同时覆盖 pause→立即 resume、F12 与控制台 pause 交错、pending 含 repeat、正常退出冲刷、SendInput 返回 0/部分数量、已消费 Up 墓碑。任何新增队列或确认机制都必须证明没有旧问题（超车、丢命令、错 ack、无限阻塞）。
- 验收：目标观察到的原始输入与回放顺序正确；暂停完成时旧 pending 已有可核查的交付结果；普通新输入与已消费 Up 的处理仍符合第三轮语义。

## 4. IF-08（必须处理后才能在 UI 中定期展示统计）：分位数持锁排序可阻塞 Hook，且报告的耗时漏掉这段等待

### 代码证据与复现

- crates/inputflow-windows/src/platform/windows.rs 第 262–282 行：callback_latency_stats() / hold_delay_stats() 持有对应 Mutex 时连续求 p50、p95、p99 和 max。
- crates/inputflow-engine/src/stats.rs 第 55–84 行：每个 p50/p95/p99 各自复制并排序最多 100,000 个样本；因此一次 stats 查询在锁内至少进行三次排序。
- windows.rs 第 234–242 行：Hook 的 finish_callback() 在返回前需要同一把统计锁，但先调用 start.elapsed()，再等待记录。锁等待不仅可能延长真实 Hook 执行时间，还不会进入本次已计算的“callback wall time”样本。hold_delay_tracker 也有类似竞争。
- 控制台命令 stats 已会触发查询；未来 UI 周期轮询会放大竞争概率。这是可行的竞争路径，**不是已经测得超时或 Hook 被移除**。

### 修复与验收

- 添加固定样本的分位计算正确性测试与可控并发测试：一个线程频繁查询，另一个线程记录；确认结果、样本总量及 Hook 热路径不会等待完整排序。避免仅测空 reservoir。
- 比较“锁内短暂复制样本，锁外排序一次再计算所有分位数”“预计算/分片统计”“记录侧 try_lock 并计数丢弃样本”等方案；选定方案需解释统计精度、CPU/内存和热路径上界。若丢采样，必须明确可观测。
- 复核计时口径：统计锁等待、SendInput、Matcher 锁等待及 CallNextHookEx 在哪些测量中可见；无法测量自身的时间不要称为完整 Hook wall time。可用独立墙钟测量或故障注入辅助验证。
- Windows 上累积足量真实样本后反复触发 stats，同时打字、点击，记录每次查询耗时、callback p50/p95/p99/max、样本数与 Hook 是否仍捕获。Microsoft 文档指出低级 Hook 超时后可能被静默移除，故不能只看一个百分位数。

## 5. IF-09（接入正式保存/自动保存前决定）：配置恢复副本的增长与选择规则

- crates/inputflow-config/src/config.rs 第 394–496 行：成功覆盖正式配置会留下唯一命名的旧 .bak 文件；当前没有保留上限或清理流程。手动低频保存尚可，UI 若每次编辑都自动保存，副本将不断增加。
- load() 第 119–156 行在正式文件无效时会扫描 .tmp/.bak，按修改时间选第一份验证通过的副本。这个选择规则是否符合“用户最后确认启用的配置”，需要与未来 UI 的保存/恢复语义相符；失败提交的 .tmp 也可能比已提交 backup 更新。
- 在修复 IF-07/08 时先做范围和容量评估，给出安全保留策略、失败恢复优先级与用户可见诊断；若实现清理，先做故障注入，保证不会删掉唯一有效副本。此项不要求写任何 UI，也不要为美观而删除备份。

## 6. Windows 真实输入验收（延续 IF-01～IF-06，不要再以 smoke 冒充）

请在仓库的既有修复记录后新增一份本轮验证记录，逐项标注“通过/失败/未执行、环境、命令或手势、预期、观察、日志位置”。用普通目标窗口和能记录 WM_KEYDOWN/UP、鼠标消息及 WM_CONTEXTMENU 的测试窗口交叉核验：

1. 已消费 Right Down → 按 F12 暂停 → 物理 Right Up；目标不应出现非预期菜单。另测控制台 pause、立即 resume、KeyMouseButton、左键。
2. KeyChord(A,B) 和 KeyMouseButton(A,Right)：按住 A 产生重复，然后失败、命中、暂停、队列满；观察字母次数、顺序及遗漏。Hold(F8) 等功能键以实机结果为准。
3. 前缀失败回放后马上跟随新物理键及 Up；必须对照目标实际事件顺序，特别验证 IF-07 跨线程暂停。
4. 普通和提升权限的前台窗口：记录 SendInput 完整/零/部分计数、目标消息、BYPASS 与 consumed Up。部分插入若不可稳定实机触发，保留故障注入结论并写明未实测。
5. 高负载、连续打字/点击、同时查询 stats：记录样本量、p50/p95/p99/max、查询耗时及 Hook 持续捕获情况。自行设定并说明可接受阈值依据，不能因为“尚无基线”而宣称 IF-06 已通过。
6. 左右修饰键、扩展键、不同键盘布局、鼠标移动后回放位置及已有物理修饰键对 action 的影响；明确当前支持边界。
7. 正常退出仍按住已消费键或右键：释放于 2 秒内和超过 2 秒两种情况；观察正常关闭、诊断与残余 orphan Up 风险。

运行并原样记录：

    cargo fmt --all -- --check
    cargo test --workspace
    cargo clippy --workspace --all-targets -- -D warnings
    cargo build -p probe-cli

如当前工具无法模拟物理键鼠，先完成全部可自动执行的工作，并给用户可操作的 Windows 人工步骤；相关项目标“未执行”，等待真实记录回填。不要凭测试通过宣称桌面行为正确。

## 7. 输出格式和完成门槛

交付时提供：

| 编号 | 根因及证据 | 方案取舍 | 修改文件/关键位置 | 自动测试 | Windows 实测 | 剩余限制 |
| --- | --- | --- | --- | --- | --- | --- |
| IF-07 |  |  |  |  |  |  |
| IF-08 |  |  |  |  |  |  |
| IF-09 |  |  |  |  |  |  |
| IF-01～IF-06 回归 |  |  |  |  |  |  |

结论要区分：

- “可以讨论 UI 设计稿”与“可以开始写不启动 Hook 的静态 UI 骨架”；
- “可以把规则启用、暂停、配置保存接入真正的 Tauri 命令”；
- “可以作为稳定的输入工具给用户持续运行”。

IF-07 的顺序正确性及 IF-08 的统计热路径处理完成后，才能考虑真实 UI 命令接线。持续运行还须本节 Windows 实测通过并记录 IF-03 的 Win32 权限边界。**不要在此任务中设计或编写前端；用户将另行决定 UI 设计方案并提交设计稿。**

## 8. 参考依据

- 低级键盘 Hook 的消息线程和超时风险：https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc
- SendInput 的插入数、顺序、UIPI 限制：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- 项目内第三轮修复记录：check-fix-debug-list/tag_3_InputFlow-M6-三轮复查修复记录.md
