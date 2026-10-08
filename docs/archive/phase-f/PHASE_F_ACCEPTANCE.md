# Phase F：鼠标方向现场验收任务

> [!CAUTION]
> 本任务已于 2026-10-08 完成并归档。项目当前入口只看
> [`../../status/CURRENT_STATUS.md`](../../status/CURRENT_STATUS.md)。
>
> 文档类型：已完成任务卡
>
> 状态：F-PHY-01～07 全部通过；完整结果见 [`PHASE_F.md`](PHASE_F.md)
>
> 实现依据：[`../../decisions/ADR-008-鼠标方向规则与直通策略.md`](../../decisions/ADR-008-鼠标方向规则与直通策略.md)

## 目标

验证已经实现的键盘激活鼠标四方向功能在真实 Windows 输入链路中的行为和短时资源边界。
本任务不再包含 F0～F3 的设计或编码；原实施任务已归档到
[`PHASE_F_IMPLEMENTATION_TASK.md`](PHASE_F_IMPLEMENTATION_TASK.md)。

使用隔离配置 `scripts/acceptance/configs/mouse-direction-f8-four.json`。操作工具和证据目录见
[`../../guides/WINDOWS_ACCEPTANCE.md`](../../guides/WINDOWS_ACCEPTANCE.md)。实际结果统一写入
[`PHASE_F.md`](PHASE_F.md)。

## 开工记录

开始前记录：

- 当前提交、分支和工作树状态；
- Windows build、会话权限、鼠标／键盘、显示器、DPI 和布局；
- Agent／Settings 实际产物及配置备份；
- 目标应用、日志目录和资源采样命令。

不得把提交 `3c3a158` 的自动结果或 Phase E 的物理结果重新写成本轮现场执行结果。

## F-PHY 矩阵

| 编号 | 操作 | 通过条件 | 最终状态 |
|---|---|---|---|
| F-PHY-01 | UI 建立同一激活键的四方向组，保存并重开 | 字段、enabled、动作和顺序读回一致 | 通过 |
| F-PHY-02 | 四方向分别越阈值；同次按住继续移动 | 每次只输出一个完整动作；真实释放后可再次触发 | 通过 |
| F-PHY-03 | 抖动、距离不足、提前释放、超时和斜线 | 与 ADR-008 一致；激活键归属正确，光标正常移动 | 通过 |
| F-PHY-04 | 候选中 F12／pause、修改配置、进入／取消预览 | 旧候选取消；无重复动作、粘键或迟到 UI 覆盖 | 通过 |
| F-PHY-05 | 持续 move 约五分钟，同时少量打字、点击和 stats 查询 | Hook 保持工作；资源有口径；无无界增长或 output failed/dropped | 通过，性能参考线例外已调查并保留 |
| F-PHY-06 | 记事本、浏览器和资源管理器中的普通输入、菜单与拖拽 | 未激活不命中；普通输入和拖拽不受影响 | 通过 |
| F-PHY-07 | 保持激活键进入暂停及正常退出 | down/up、drain 和 release tombstone 符合 M6 语义 | 通过 |

## 证据要求

- 记录真实手势、目标应用、预期、实际、用户可见现象、日志路径和结论。
- 资源至少包含 observed／callback 样本增量、p50／p95／p99／max、stats RTT、CPU 时间差、
  working set、private bytes、线程、句柄及 failed／dropped 增量。
- 本机短测参考线：stats p99 < 20 ms、callback p99 < 5 ms、max < 100 ms。超线时调查原因，
  不修改数字或隐藏尖峰。
- 用真实物理输入的观察计数、目标消息和规则命中证明 Hook 在测试期间仍工作；进程存活或 IPC ready
  不能单独证明 Hook 存活。
- 没有 1000 Hz 设备或第二屏时，按实际硬件执行并记录未验证范围；脚本注入不得冒充物理手势。

## 完成门槛

- [x] F-PHY-01～07 均有现场记录并通过。
- [x] 受 matcher／Windows Hook 改动影响的 M6 和 Phase E 路径完成必要现场回归。
- [x] 完整联合构建通过；现场修复保留修复前失败和修复后新证据。
- [x] Phase F 记录、当前状态和根 README 摘要同步。
- [x] 本文件与 Phase F 记录归档；下一入口切换到 G-PRE。
