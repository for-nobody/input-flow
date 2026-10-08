# InputFlow 开发步骤（Steps.md）

> [!CAUTION]
> 历史全阶段清单，已由 [`../../planning/IMPLEMENTATION_STEPS.md`](../../planning/IMPLEMENTATION_STEPS.md)
> 取代。下方旧勾选、路径和阶段入口只用于追溯，不是当前进度；当前状态见
> [`../../status/CURRENT_STATUS.md`](../../status/CURRENT_STATUS.md)。

> 本文件是 InputFlow 的**可执行开发计划**，把 `InputFlow-项目规划.md`（下称"规划稿"）拆解成可逐步执行、可勾选、可验收的动作清单。
> 开发时按里程碑顺序执行，**每次只做当前里程碑**。
>
> 2026-09-27 架构修订：M7 使用纯 Rust + Win32 常驻 agent、Windows Named Pipe 和按需启动的 C# + WinUI 3 设置程序。Tauri 2/React/Node.js 不再是项目技术栈。

## 本轮剩余工作入口（2026-10-08）

M6 与 Phase E 已完成，保留下方历史清单供查阅。Phase F 的 F0～F3 代码和自动验证已完成，当前从 F4 真实 Windows 输入验收继续，再按 F5 → G-PRE → H → RC → 首个 Pre-release → G-POST 执行；鼠标方向必须在首版，24／72 小时长测及长期 daily-drive 在首版发布后。以 `docs/RELEASE_ROADMAP.md`、tag_6 任务和本文件 Step 8 以后为准，不重新从旧里程碑开工。

## 使用约定

- 必读前置材料（开工前先读）：`InputFlow-项目规划.md`、仓库 `README.md`、`docs/decisions/` 下已有 ADR、`docs/research-log.md`、以及当前已有代码。
- **语言约定**：
  - 本文档（Steps.md）以及项目文档、规划、ADR、研究日志使用**中文**书写。
  - 程序代码中的**注释、标识符、提交信息（commit message）、测试名称**一律使用**英文**书写。
- 每次只实现当前里程碑；开始前先说明本次要验证的具体行为。
- 对状态/顺序/回放的改动，先写最小事件序列测试；Windows 特有行为必须提供实机复现步骤，不能用 Linux 或纯单测代替验证。
- 交付每个阶段时，报告：改动文件、验证命令、Windows 实机结果、尚未验证的边界、下一阶段依赖。
- Hook 回调内只做事件归一化、极轻量的规则前缀/状态判定和有界入队；不得执行 GUI、磁盘 I/O、网络访问或等待工作线程。
- 所有 `unsafe`、Win32 资源清理和线程生命周期集中在 `platform/windows` 模块，并说明不变量。
- 配置保持向后兼容或提供显式迁移；不自动提升权限、不安装驱动、不修改系统级输入设置。

## 全局测试纪律

- **纯引擎测试（必须自动化）**：事件前缀不存在、规则匹配/失败/临界超时、多候选竞争、键按下/释放次序、自动重复 down、鼠标双击、暂停、配置替换、同键冲突检查、队列满、计时器重复触发、回放事件顺序与归属、动作最多执行一次。
- **Windows 实机测试（手动或半自动）**：记事本/浏览器/资源管理器中打字、快捷键、拖拽、右键菜单；触发后继续按住左 Ctrl、快速连击、同时按其他修饰键、切换前台窗口；普通与提升权限窗口；暂停/关闭/强杀/坏配置/Hook 安装失败/`SendInput` 返回不足；与输入法及其他改键软件共存。
- 每个缺陷至少记录：Windows 版本、规则、输入事件顺序、目标应用、预期/实际结果、是否丢键/卡键。
- 性能报告给出样本量与测量方法（p50/p95/p99）。

## 交付报告模板（每个里程碑结束时填写）

```text
- 改动文件：
- 验证命令：
- Windows 实机结果：
- 尚未验证的边界：
- 下一阶段依赖：
```

---

## Step 0（M0）：环境与仓库准备

**目标**：固定开发环境、确认工具链可编译最小 Rust 程序、建立仓库骨架与术语表、记录研究结论、建立 ADR 流程。

### 0.1 记录开发机环境
- [ ] 记录 Windows 版本、CPU 架构、Rust 工具链版本、MSVC/C++ 构建工具版本。
- [ ] 将上述信息写入 `README.md`（满足 NFR-06：首次实现时固定环境）。

### 0.2 验证最小程序可编译
- [ ] 新建最小 Rust 程序（`cargo new`），确认可编译运行。
- [ ] 引入最小 `windows`（或 `windows-sys`）crate 调用并锁定版本，确认可编译运行。
- [ ] 把实际锁定的依赖版本写回 `README.md`。

### 0.3 建立仓库骨架
- [ ] 决定 M1 用单包起步还是直接建 workspace（按规划稿 7.1：M1 可单包，待引擎与接入需独立测试时再演进为 workspace）。
- [ ] 创建 `docs/`、`docs/decisions/` 目录。
- [ ] 将 `InputFlow-项目规划.md` 整理/复制为 `docs/PROJECT_PLAN.md`（作为项目上下文留存）。

### 0.4 术语表与研究日志
- [ ] 在 `docs/` 建立术语表（glossary）：明确"物理按住/可见按键状态""暂扣""回放""消费""旁路""候选前缀"等术语。
- [ ] 建立 `docs/research-log.md`，记录四条主线（Windows 输入链路、已有工具、算法、交互）的调研结论，不堆功能数量。

### 0.5 首个 ADR
- [ ] 在 `docs/decisions/` 建立 ADR 模板。
- [x] 写 ADR-000：仓库结构 + 初始技术选型；其 GUI 选型已由 ADR-004 取代，其余 Rust/Win32 决策继续有效。
- [x] 写 ADR-004：纯 Rust 常驻 agent + C#/WinUI 3 设置程序 + Named Pipe。

### 验收
- [ ] `README.md` 包含系统/工具链/依赖版本。
- [ ] 最小程序编译运行成功。
- [ ] `docs/`、`docs/decisions/`、`docs/research-log.md` 就位。

---

## Step 1（M1）：只读输入探针

**目标**：在 Windows 上建立最小 Rust 控制台程序，安装键盘/鼠标低级 Hook 和消息循环，只打印事件；不抑制、不回放、不建 GUI。

### 1.1 创建探针程序
- [x] 在 `apps/probe-cli/`（或单包）建立控制台程序。
- [x] 用 windows-sys crate 安装 `WH_KEYBOARD_LL` 与 `WH_MOUSE_LL` Hook。

### 1.2 消息循环与回调
- [x] 建立专用消息循环线程；Hook 回调内只做归一化与非阻塞有界入队，独立 logger 线程打印，快速返回。
- [x] 实现退出时卸载 Hook（控制台输入 `quit`/`exit`/`q` 或 EOF），保证清理顺序正确。

### 1.3 事件打印
- [x] 打印键盘 down/up、左右 Ctrl、普通键、鼠标左右键、滚轮。
- [x] 打印时间戳与 `injected` 标志。
- [x] 打印事件先后顺序。

### 1.4 实机验证
- [x] 在 Windows 实机验证目标输入顺序正确（捕获物理鼠标/滚轮事件顺序、injected=false）。
- [x] 验证退出行为正常（Hook 正确卸载、程序以退出码 0 干净退出）。
- [x] 把观察结果写入 `docs/research-log.md`。

### 1.5 决定 M2 细节
- [x] 根据观察结果，确定 M2（抑制与回放）的具体实现方式（见 `research-log.md` 的 M2 方向）。

### 验收
- [x] 控制台正确打印键鼠事件（含左右 Ctrl、鼠标键、滚轮、时间戳、injected）。
- [x] 无任何拦截/改键副作用。
- [x] 退出干净、Hook 卸载无残留。
- [x] 结果写入 `research-log.md`。

---

## Step 2（M2）：抑制与回放探针

**目标**：只暂扣 F8，短暂延迟后回放；验证抑制、`SendInput` 回放、注入不递归、可立即关闭。

### 2.1 抑制 F8
- [x] 在 Hook 回调中对 F8 down 返回非零以抑制原始事件。
- [x] 只暂扣 F8，其余键直接放行。

### 2.2 延迟回放
- [x] 用 `SendInput` 短暂延迟后回放 F8，检查实际插入数量与返回值。
- [x] 记录失败并进入可观察的旁路状态。

### 2.3 注入识别
- [x] 给注入事件带本程序 `dwExtraInfo` 标记，结合 Hook 的 injected flag 识别本程序生成的事件。
- [ ] 验证注入不递归（回放的事件不会再次触发规则）。

### 2.4 紧急关闭
- [x] 实现"立即关闭/旁路"手段（如另一紧急键），可随时停止拦截。

### 2.5 实机验证
- [ ] 验证 F8 只到达目标应用一次。
- [ ] 验证权限失败（如提升权限窗口）可观察。
- [x] 记录结果到 `docs/research-log.md`。

### 验收
- [ ] F8 只到达应用一次；注入不递归。
- [ ] 紧急关闭随时有效。
- [x] `SendInput` 返回值被检查，失败可观察。

---

## Step 3（M3）：核心状态模型

**目标**：建立 `InputEvent`、物理/可见按键状态、有界暂扣队列、定时与回放计划；纯引擎可确定性测试。

### 3.1 演进为 workspace（如 M1 未建）
- [x] 建立 Cargo workspace：`crates/inputflow-engine`（纯逻辑）与 `crates/inputflow-windows`（平台接入）。
- [x] `inputflow-engine` 不依赖 Windows API，可跨平台单测。

### 3.2 事件模型 `engine/event`
- [x] 定义 `InputEvent`：事件来源（键盘/鼠标）、虚拟键与扫描码、左右/扩展键标记、down/up、单调时间戳、顺序 ID、注入标记、鼠标按键/滚轮数据。
- [x] 不把 `physical=true/false` 简化成可信设备身份。

### 3.3 暂扣队列 `engine/pending`
- [x] 有界暂扣；down/up 配对；回放次序；溢出策略（到上限旁路新事件并尽力冲刷已有事件）。

### 3.4 按键状态模型
- [x] 维护"物理按住"与"目标已看到"两套状态。
- [x] 保证任何已消费的 down 不会把其 up 裸露给应用（满足 NFR-04）。

### 3.5 匹配器与接口 `engine/matcher`
- [x] 纯逻辑状态机 + 可注入逻辑时钟（测试不依赖真实睡眠）。
- [x] 定义 `Decision`（`PassThrough` / `Suppress{event_id}`）与 `Resolution`（`Pending` / `Matched` / `Failed{replay}`）。
- [x] `on_event` 同步返回当前事件处理决定；`on_timeout` 在期限到达时推进状态并产出后续命令。

### 3.6 单元测试
- [x] 覆盖按下/松开、超时、溢出、暂停/切换、回放计划事件顺序、down/up 归属。

### 验收
- [x] `inputflow-engine` 不依赖 Windows，纯单测全部通过。
- [x] 队列有界、溢出有旁路策略。
- [x] down/up 状态一致，无孤立释放。

---

## Step 4（M4）：组合匹配

**目标**：实现 `Key+Key`、`Key+MouseButton` 组合匹配，无候选直通，失败回放，命中消费。

### 4.1 规则模型与预编译
- [x] `engine/rules`：规则校验、冲突检测、预编译成回调可快速查询的形式。
- [x] 支持 `Key+Key`、`Key+MouseButton`。

### 4.2 前缀匹配与直通
- [x] 只暂扣可能构成已启用规则前缀的事件。
- [x] 无候选规则的键直接放行；禁用全部规则时不暂扣。

### 4.3 失败/超时回放
- [x] 匹配失败或超时后按序回放暂扣事件（例如 Ctrl+Q 失败时按序送出 Ctrl、Q）。

### 4.4 命中消费
- [x] 命中后消费触发输入并发送动作（例如 Ctrl+右键命中且原右键菜单不弹出）。
- [x] 动作仅触发一次。

### 4.5 冲突检测（初步）
- [x] 对冲突规则先拒绝并解释原因。

### 4.6 测试
- [x] 单元测试 + 实机测试：Ctrl+Q 失败回放；Ctrl+右键命中。

### 验收
- [x] Ctrl+Q 失败时按序回放，无丢键/卡键。
- [x] Ctrl+右键命中，无原右键菜单，动作只触发一次。
- [x] 无候选输入直接放行。

> 注：4.6/验收中的组合交互（Ctrl+右键命中、Ctrl+Q 失败回放）已由纯引擎事件序列单测覆盖；平台侧已通过 `echo quit | probe-cli.exe` 实机回归（Hook 安装、退出码 0）。逐键/逐击的交互实机验证复现步骤已写入 `research-log.md`，需人工按键确认。

---

## Step 5（M5）：时序规则

**目标**：实现 `Hold` 与 `Hold+MouseButton` 及独立长按单键；处理阈值边界、重复键、冲突。

### 5.1 Hold(K,T)
- [x] 从物理 K down 到计时达 T；按键重复 down 不重置计时；T 前释放则不命中。

### 5.2 Hold(K,T)+Button(B)
- [x] 达 T 后且 K 仍物理按住，随后 B down 才命中；本次 B up 与已消费 down 一同处理，避免孤立释放。
- [x] MVP 不允许"先按 B 再达到 T"。

### 5.3 独立长按单键
- [x] 实现独立 `长按左 Ctrl → Ctrl+C` 规则。

### 5.4 冲突策略
- [x] 单键长按与同前缀复合规则同时配置时，判为冲突并拒绝启用。

### 5.5 测试
- [x] 阈值边界、重复键、松开、冲突规则测试。

### 验收
- [x] 阈值内/外及不同第二输入产生预期判定。
- [x] 独立长按与复合规则冲突时分别启用，MVP 不暗中选优先级。
- [x] 单元测试覆盖阈值边界与重复键。

---

## Step 6（M6）：可靠性

**目标**：暂停/旁路、配置验证、诊断日志、异常恢复；无效配置可启动。

### 6.1 暂停/旁路
- [x] 可配置紧急组合、退出命令、暂停/恢复机制（先冲刷已暂扣再切换）；紧急组合不与用户规则冲突。（托盘暂停推迟到 M7）
- [x] 暂停时不创建新暂扣，清理现有状态后切换。

### 6.2 配置验证
- [x] `config`：版本化配置、原子保存、坏文件回退。
- [x] 校验类型、阈值范围、重复 ID、保留组合、规则冲突；只把完整有效规则集交给引擎。
- [x] 缺失或无效配置不阻止启动（默认无规则旁路）。

### 6.3 诊断日志
- [x] 记录匿名化运行诊断：回放失败、超时、队列溢出、hook 状态。
- [x] 默认不记录实际输入文本、密码或窗口标题；调试采集须明确开启（`--debug`）。

### 6.4 异常恢复
- [x] Hook 安装失败、`SendInput` 返回不足、崩溃/强杀后的提示。
- [x] 进程异常终止后，启动时提示上次异常（不承诺恢复已暂扣输入）。

### 6.5 性能采样
- [x] 记录直通回调耗时与暂扣总延迟的 p50/p95/p99（`stats` 命令与退出时报告）。

### 验收
- [x] 无效/损坏配置可启动且默认旁路。
- [x] 用户在规则出错后仍可关闭拦截（紧急键 / `pause` / `quit`）。
- [x] 日志默认不含敏感文本。
- [x] 有性能采样机制（真实基线数据待交互实机采集）。

> 注：6.1 的「托盘暂停」按确认推迟到 M7；M6 在 probe-cli 控制台实现暂停机制 + 可配置紧急组合 + 退出命令。性能采样机制已实现并报告 p50/p95/p99/max，但自动回归未产生输入事件（样本 0），真实基线数据需人工按键实机采集（见 `research-log.md` 第 10～11 节）。

> 2026-09-27 三轮复查：IF-01～IF-05 已完成针对性代码加固和故障注入/事件序列测试；IF-06 的同步 `SendInput` 最坏 Hook 耗时与 Hook 存活仍没有真实输入样本。M7 的视觉设计可推进，但规则创建/启用、暂停和持久化的正式 UI 接线须等待 `check-fix-debug-list/archive/tag_3_InputFlow-M6-三轮复查修复记录.md` 中的 Windows 人工矩阵通过。

> 2026-09-27 四轮复查：IF-07 已把外部暂停/恢复调度到 Hook 消息线程并同步确认回放结果；IF-08 已改为统计锁内单次快照、锁外单次排序，并修正计时口径；IF-09 已固定“已提交 backup 优先于未提交 temp”并实施有效性保护的 5/3 代保留。自动测试与无输入 Windows lifecycle smoke 通过；真实键鼠、UIPI 和高负载 Hook 存活矩阵仍待人工回填，详见 `check-fix-debug-list/archive/tag_4_InputFlow-M6-四轮后端修复记录.md`。

---

## Step 7（M7）：原生产品外壳与完整键盘

**目标**：将原型拆为纯 Rust + Win32 常驻 agent 与按需启动的 C# + WinUI 3 设置程序，以版本化 Named Pipe 通信；补齐完整键盘的捕获、显示、配置和回放。设置程序不持有 Hook，关闭窗口后进程完全退出。

**进入条件**：先完成 M6 四轮复查记录中的 Windows 菜单/重复键/UIPI/高负载性能验收；未通过前只做不会启用真实拦截规则的视觉与工程骨架。该进入条件已于 2026-10-01～02 的联合实机验收中收口，partial SendInput 保留明确限制。

### 7.1 决策与构建基线
- [x] 接受 ADR-004：Rust agent + WinUI 3 设置程序；不使用 Tauri、React、Node.js 或 WebView。
- [x] 按 `docs/BUILD_WINDOWS.md` 记录 Visual Studio、.NET SDK、Windows SDK、Windows App SDK、WinUI 3 模板和真实构建命令。
- [x] 先建立不启动 Hook 的 WinUI 3 smoke 窗口，验证构建、启动和关闭后进程退出。

> 2026-09-29 Phase A：在 x64 本机验证 unpackaged + framework-dependent WinUI 3 smoke 的 restore、Debug/Release build、原生窗口启动和正常关闭后进程退出。页面未接 Hook、配置或 agent。self-contained 已作为 fallback 验证；packaged 构建成功但因当前会话无法启用 Developer Mode 而未验证启动。证据见 `check-fix-debug-list/tag_5_InputFlow-M7-WinUI3架构与输入扩展记录.md`。

### 7.2 Rust agent 与 Named Pipe
- [x] 建立 `apps/inputflow-agent/`，复用现有 engine/config/windows crate，不复制 Hook 实现。
- [x] agent 是唯一 Hook、托盘、规则验证/保存/应用和正式配置所有者。
- [x] 定义版本化 IPC：handshake、status、validate/apply、pause/resume、recording、diagnostics、request ID、超时与错误码。
- [x] Named Pipe 只允许当前交互用户访问；断开设置程序不影响 agent 和已启用规则。

> 2026-09-30 Phase C：新增 probe/agent 共用 `inputflow-runtime`，实现 Hook-owner 规则热替换、结构化 save/apply、capture session、Win32 托盘/Explorer 恢复、单实例和无控制台 Release。后续审阅又移除了独立托盘暂停状态，并为热替换增加取消/确定失败/结果待定三态与延迟完成核对。128 项自动测试、lifecycle/resource smoke、Active/Pause/Resume/Open Settings/Exit 人工托盘验收，以及 F12 后 tooltip/菜单立即显示 Paused/Resume 并可恢复 Active 的人工复验均通过；2026-10-01 的真实 Explorer 重启进一步确认图标与菜单自动恢复。版本化 Named Pipe 明确保留给 Phase D。

> 2026-09-30 Phase D：ADR-006 固定 4-byte little-endian length-prefix + UTF-8 JSON、1 MiB 上限、协议 v1、string request ID、重复拒绝、超时/断线/关闭语义和有界推送。agent 已接入当前 session pipe；每个 instance 使用当前用户 + LocalSystem protected DACL、拒绝 remote client 和 overlapped I/O。Rust 实际 pipe 测试覆盖部分帧、损坏/超长、版本、并发、超时放弃、断线、重连、重复 ID、订阅与 shutdown；C# client/golden contract 及 live agent 的 status/config/validate/apply/pause/resume/stats/capture、事件推送和 capture-owner 断线取消均通过。正式页面仍属于 Phase E。

### 7.3 完整键盘与 Schema v2
- [x] ADR-005 明确 logical VK 与 physical scan code + extended、布局显示、输出和 Unknown 策略。
- [x] 自动映射/往返覆盖 Caps/Num/Scroll Lock、导航/方向、OEM、numpad/keypad Enter、PrintScreen/Pause/Menu 和常见媒体键。
- [x] Schema v2 严格表达 match mode；v1 可读、内存迁移、v2 保存和 v1 committed-backup 回滚均有 golden 测试。
- [x] Caps Lock 自动测试覆盖无规则、候选失败、命中、repeat、pause/F12/quit 共用冲刷、overflow 和精确一对 down/up scan 回放。
- [x] 用 en-US 和 Microsoft Pinyin 完成真实 OEM 观察/目标显示与 physical 失败回放/命中，并验证 Caps Lock 失败回放、命中消费、repeat、`F12` pending 恢复及键盘指示状态；完整序列见 tag_5 执行记录。正式设置 capture/display session 仍属于 7.2/7.4。

### 7.4 WinUI 3 设置程序
- [x] 建立 `apps/settings-winui/` 的 C# + WinUI 3 工程，不实现第二套 Hook 或规则校验器。
- [x] 提供规则列表、编辑、启停/删除、运行状态、冲突/延迟提示、诊断与恢复入口。
- [x] 输入录制必须显式开始、超时并可取消；通过 agent 捕获，始终保留 F12 紧急旁路。
- [x] UI 只提交草稿；agent 验证、原子保存并在成功后热应用，失败返回结构化错误且保持旧规则。

> 2026-09-30 Phase E 实现：ADR-007 / Schema v3 为规则加入持久化 `enabled`；新增 `InputFlow.Settings.Core`、应用级 control/event 协调、草稿/并发检查、validate/apply/reconciliation、capture session 过滤和正式 WinUI 页面。2026-10-01～02 联合验收又完成页面物理 capture、三类规则与同步、UIA/高对比度/缩放、五分钟资源及 M6 矩阵，并修复录制启动竞态、规则开关刷新、输出重入/UIPI 误报和响应式宽度；Phase E 以明确硬件/环境限制收口。

### 7.5 资源与联合验收
- [x] 设置窗口关闭后 UI 进程完全退出；agent 不加载 .NET、WinUI 或 WebView。
- [x] agent 空闲/Hook 活跃/鼠标高频移动时记录 CPU、工作集、线程、句柄和回调分位。
- [x] 无需手改 JSON 即可完成示例规则与暂停；冲突/非法配置给出可理解错误。
- [x] 可录制并往返 Caps Lock、一个 OEM 符号、导航键和媒体音量键；keypad Enter/独立播放键由 selector/contract 覆盖，本机无相应硬件，未冒充物理实测。
- [x] Rust、WinUI 和联合构建命令可重复，Windows 实测与未验证项写回 README/BUILD_WINDOWS。

---

## Step 8（M8／Phase F）：首版鼠标方向

**主任务**：`check-fix-debug-list/tag_6_InputFlow-Phase-F-鼠标方向实施任务.md`。

**目标**：键盘激活的四方向规则、同键四方向组、一次按住最多一次；鼠标移动始终直通；动作沿用 KeyChord。

- [x] F0：核对真实基线，创建 ADR-008；固定激活键 Down／repeat／Up、坐标／单位／时间、偏轴、冲突和 Schema。
- [x] F1：确定性算法／状态机测试与实现；覆盖抖动、边界、折返、超时、提前释放、负坐标、注入和重复触发。
- [x] F2：无逐点分配的固定大小 move 路径；不进 PendingQueue；SendInput 锁外输出和 owner 串行化不变。
- [x] F3：版本化配置、v1／v2／v3 迁移、Rust／C# fixture／handshake 及 WinUI 编辑／保存／有限预览。
- [ ] F4：真实四方向、取消、暂停／替换和至少五分钟物理 move 短负载；硬件限制如实记录。
- [ ] F5：完整构建／必要自动回归和文档已更新；待 F4 后补齐受影响 M6 现场路径，才能最终勾选。

2026-10-08 自动结果：ADR-008、Schema v4、协议 capability、WinUI 编辑／有限预览和方向 matcher／Hook 已落地；Rust 169／169、C# protocol 6／6、Settings Core 11／11 通过，WinUI Debug／Release 与联合构建通过。合成 125／500／1000 Hz 序列只证明确定性有界行为，不代表真实设备 polling rate；F-PHY-01～07 仍未执行。

**完成门槛**：tag_6 F 清单满足后，首版范围 feature-complete，进入 Step 9；不包含长测或安装器等待条件。

设备区分、鼠标按钮激活、序列／层、按应用规则、原生触控板、URL／启动程序留作后续独立任务，不扩大本轮首版范围。

---

## Step 9（G-PRE）：发布前有限可靠性

**主任务**：`check-fix-debug-list/tag_6_InputFlow-首个Release收尾与发布任务.md` 的 G-PRE。

- [ ] 新提交自动回归：Rust、fmt、Clippy、Agent Release、WinUI Debug／Release、contract／Core。
- [ ] 一次约 10～15 分钟混合物理输入，记录 Hook 工作证据与资源；不要求长时间连续挂测。
- [ ] 有限 UI 开关、规则应用、Pipe 断线／重连、UI 异常结束／重开、Agent 正常／异常恢复。
- [ ] F12／托盘／UI、候选中 pause／replace／quit 释放归属及实际可用锁屏／睡眠恢复。
- [ ] 已发现核心输入／配置阻断缺陷修复；未测硬件、预期 UIPI 拒绝和故障注入分开记录。

**完成后**进入 Step 10，不等待 24／72 小时长测或几天 daily-drive。

---

## Step 10（H）：分发与用户生命周期

按首版收尾任务的 H 执行；默认优先验证 unpackaged x64 便携目录分发，最终方案写 ADR。

- [ ] ADR-009（或下一空闲编号）固定分发／依赖／路径／权限／升级和移除方案。
- [ ] 新增统一 release 打包入口；构建失败非零退出；明确 .NET 与 Windows App SDK 两种 self-contained 属性。
- [ ] 核对裁剪与原生运行依赖，产物不要求开发机 SDK，不假定仅拷两个 exe 就可运行。
- [ ] 布局兼容 Agent 查找 Settings；空格／中文路径、任意工作目录、双单实例和默认数据目录可用。
- [ ] 可选当前用户 Agent 自启动默认关闭；启用／禁用／状态／升级入口有实际行为和说明。
- [ ] 升级保留配置及禁用状态，Schema／降级有明确策略；移除不删共享运行库，默认保留个人配置。
- [ ] 干净环境完成 DIST-01～07；支持 OS／架构范围以证据为准。
- [ ] README／使用说明、依赖、暂停／退出、自启动／更新／移除、日志／配置恢复说明齐全。

独立 installer、MSIX、签名和商店不是默认首版硬性条件；实际承诺的分发流程必须验收。没有干净机证据时不能勾选分发完成。

---

## Step 11（RC）：最终产物和发布准备

- [ ] 固定实际提交与版本；Agent／UI／包／关于页元数据一致。
- [ ] 最终包通过有限 smoke，记录命令、包大小、SHA-256 和目标环境。
- [ ] 发布说明、CHANGELOG、用户指南、项目许可证决定及第三方 NOTICE 齐全。
- [ ] 发布草稿建议 `v0.9.0-beta.1` Pre-release，明确长测未完成和全部已知限制。
- [ ] 附件无用户正式配置、token、完整输入日志或开发机私有路径。
- [ ] 可审阅包／草稿完成后按已有对应授权执行远端发布，或准确交付“发布准备完成、待发布”。

RC 是有限检查，不增加数天自用或长测前置条件。

---

## 首个 Release 发布门槛（取代旧 M1–M7 MVP 清单）

- [ ] 鼠标方向 Phase F 已完成，并有真实四方向及短时资源证据。
- [ ] G-PRE 通过，核心输入／保存／恢复没有未解决的可复现阻断问题。
- [ ] H 的实际分发、依赖、自启动、升级和移除在声明环境可用。
- [ ] RC 最终包 smoke、版本、校验和、许可及说明准确。
- [ ] 首版定位公开测试版本，README 不宣称已完成全天候长期稳定验收。
- [ ] 实际 release 已按授权发布并核对提交／附件，或如实停在已完成的发布准备。

**24／72 小时长测不是本门槛的一部分。** 已通过的历史基线不能自动勾选新代码或最终包。

---

## Step 12（G-POST）：首版发布后的长测与加固

**主任务**：`check-fix-debug-list/tag_6_InputFlow-发布后Phase-G长时间运行任务.md`。**进入条件：首版已实际发布。**

- [ ] 基于已发布版本／包校验和建立 session，不混用 Debug 或后续变更产物。
- [ ] 长测工具支持有界超时、断线／重连、实例身份、分段轮转、取消和记录覆盖率。
- [ ] 逐步记录 POST-A、24h、72h；区别墙钟／进程存活／采样／睡眠／真实输入时段。
- [ ] 日常输入、UI／配置、锁屏／唤醒和真实 Hook 工作有观察；资源增长及日志成本有分析。
- [ ] 缺陷保留旧版失败证据，修复／短测后发布补丁，新版另建 session。
- [ ] 根据长期与环境证据推进 v0.9.x，日后再决定 v1.0 行为及 Schema 稳定承诺。

本步骤未完成不阻塞首次 release；发现的真实严重问题应及时修复，不必等满 72 小时。
