# 首版发布 G-PRE 完成记录

> 文档类型：历史执行快照
>
> 状态：已完成并归档
>
> 执行日期：2026-10-09（Australia/Brisbane）
>
> 任务卡：[`../../tasks/FIRST_RELEASE.md`](../../tasks/FIRST_RELEASE.md)

## 1. 基线与范围

- 分支：`main`，提交 `001e4ad8dbf1137ed114425e1953a7dceb7469c0`，与 `origin/main` 一致。
- 平台：Windows NT `10.0.26300.0`，x64；Rust/Cargo `1.99.0`；.NET SDK `10.0.401`。
- G-PRE 只验证发布前有限可靠性，不包含 H、RC、远端发布或发布后 24／72 小时 G-POST。
- 故障注入和配置写入全部使用 `target/acceptance/g-pre/` 下的隔离目录，没有覆盖用户正式
  `%LOCALAPPDATA%\InputFlow\config.json`。
- 开始时保留用户已有的 `docs/status/CURRENT_STATUS.md` 基线更新和未跟踪触控板提案；二者不作为
  产品源码输入。

## 2. 问题与最小方案

目标是证明同一固定提交的新构建产物通过自动门槛、约 10～15 分钟真实混合输入、UI／Pipe／
配置生命周期、暂停恢复、异常恢复和退出边界。执行顺序为：完整构建 → live contract 与隔离 smoke
→ 自动故障矩阵 → 物理输入与资源采样 → 针对缺口补充现场验证。没有为收尾重构已通过架构。

## 3. 自动门槛

| 检查 | 结果 | 证据 |
|---|---|---|
| `scripts/build-windows.ps1` 完整链 | **通过** | fmt、171／171 Rust、严格 Clippy、Probe／Release Agent、WinUI Debug／Release 0 warning／error、C# protocol 6／6、Settings Core 11／11 |
| 真实 Agent live contract | **通过** | `target/acceptance/g-pre/live-contract/`；fixture 6／6，在线 status／config／validate／apply／pause／resume／stats／capture／disconnect 通过；PID 2832 正常退出 0 |
| 隔离 smoke | **通过** | `target/acceptance/g-pre/smoke-confirmed/`；10 次 pause／resume、replace、capture cancel，cleanup warning 0，正常退出 0，Hook／logger 无 panic |
| 20 次 Settings 周期 | **通过** | `settings-cycles-confirmed/`；20／20 正常退出，Agent PID 5216 不变；handle 101→101、thread 8→7、private 1.43→1.41 MiB |
| 10 次配置 apply | **通过** | `apply-cycles-confirmed/`；10／10 `applied`，启用规则数 2／1 交替正确，5 个备份、0 temp、无 recovery |
| Settings 异常结束／重开 | **通过** | 只强制结束已核验 PID 2672；Agent PID 6572 保持 ready；PID 1376 重开并正常退出 0 |
| Agent 异常恢复 | **通过** | 只强制结束已核验 PID 11064；重启 PID 9564 报告 `previous_abnormal=true`，正常退出 0 后 marker 清除 |

直接运行 `.ps1` 曾被本机 execution policy 拒绝，因此只对构建子进程使用
`powershell.exe -ExecutionPolicy Bypass`，没有修改机器或用户策略。沙箱内第一次 restore 因网络限制失败；
获准访问 NuGet 后从头重跑完整脚本，上表仅采用完整成功轮次。两个早期测试夹具的 60 秒／30 秒
期限不足，分别在第 20 次 UI 周期前和第 7 次 apply 前正常超时；二者均未计为通过，而是在新隔离目录
延长期限后从第 1 次完整重跑。

产物 SHA-256：

- `inputflow-agent.exe`：`FA5986C73BE007D157140A876414380CE90649767DC4BFBB520DF9E81D7CE249`
- `InputFlow.Settings.exe`：`AD81D7C3E2E9239C0516E9E58E4C77A8189EF785D22C3045893E0DFAB2E64C6C`

两者均为 PE x64 `0x8664`。Agent manifest 回读为 `asInvoker`、`uiAccess=false`、PerMonitorV2；
Settings 为 PerMonitorV2，product version 为 `0.1.0+001e4ad8…`。

## 4. PRE-01～09 结果

| 编号 | 结果 | 证据与结论 |
|---|---|---|
| PRE-01 | **通过** | `physical-session-20261009/`；真实目标持续 17 分 39 秒，资源采样 15 分 53 秒。用户确认 Ctrl+C／V／Z／Y、普通输入、菜单和拖拽无明显丢键、粘键或漏动作 |
| PRE-02 | **通过** | 20 次 UI 周期均完全退出，Agent PID 持续，线程／句柄无逐次增长趋势 |
| PRE-03 | **通过** | 10 次自动 apply、现场三次热替换及 UI 编辑／保存均 settled；同提交 Phase F F-PHY-04 已精确证明候选中 live replace 的 1／1 replay 和旧候选清除 |
| PRE-04 | **通过** | Settings 异常结束／重开、live owner disconnect／reconnect 通过；现场三次 capture 与后续保存未发生迟到覆盖，最终状态 `capture_active=false` |
| PRE-05 | **通过** | 正常退出、重启、隔离异常 marker 恢复及单实例边界均通过 |
| PRE-06 | **通过** | 现场 F8 候选中 F12 回放 1／1；托盘 pause／resume；补充会话 UI pause 21 秒后 resume，暂停时无动作，恢复后四方向继续完整输出 |
| PRE-07 | **通过（可用范围）** | 用户执行 Win+L 锁屏／解锁并确认恢复后无过期动作或异常触发；本轮未执行睡眠／唤醒，按环境限制如实保留 |
| PRE-08 | **通过（可用范围）** | 普通完整性目标收到完整 Ctrl 动作；物理会话 269／269、补充会话 26／26 完整，failed/dropped=0；确定性测试覆盖完整／零／partial 与 UIPI 预检。本轮未另启提升目标 |
| PRE-09 | **通过** | 最终真实 F8 按住期间退出；两秒 drain 内收到真实 Up，自行停止；无 `shutdown_limit`、孤立 Up 或线程 panic |

候选替换补充夹具第一次把 `WaitTimeoutMilliseconds` 错设为 600000，超出脚本允许的 300000，
因此没有产生产品请求，未计入产品失败。修正后的补充请求在检测 F8 后 250 ms 发起，但 500 ms 候选
已经先超时回放，故该轮只证明 apply 成功，不冒充并发候选证据；PRE-03 的该精确路径采用同提交、
同实现的 Phase F F-PHY-04 可信现场证据。

## 5. 物理会话与资源

主会话：Agent PID 5184、Settings PID 5416、目标 PID 8272；隔离配置包含 F8 四方向
80 px／500 ms／40 px。目标记录 1,010 行消息；Agent 观察 6,067 个事件、252 个方向候选，命中
Left 37、Right 56、Up 41、Down 41。共发送 269 个输出 batch，全部 `Complete`。

| 指标 | 结果 |
|---|---|
| 资源样本 | 941 个、953.320 秒、错误行 0 |
| stats RTT | p50 5.947 ms、p95 18.736 ms、p99 151.274 ms、max 1064.207 ms |
| callback 累计最终值 | 47,358 samples；p50 8 µs、p95 70 µs、p99 340 µs、max 17.265 ms |
| Agent CPU | 增量 26.766 s；按单核口径约 2.81% |
| Agent working set | 10.45→16.77 MiB，max 16.77 MiB |
| Agent private | 1.93→3.83 MiB，max 4.05 MiB |
| Agent thread／handle | 12→10（范围 9～15）／167→219（max 221） |
| Settings CPU | 增量 85.406 s；按单核口径约 8.96% |
| Settings working set／private | 144.81→205.10 MiB（max 227.31）／46.86→81.47 MiB（max 89.47） |
| 输出 | 主会话 269 sent、0 failed、0 dropped；补充会话 26／0／0 |

stats RTT 的 p99／max 超过 `<20 ms` 本机短测参考线，但 callback p99／max 明显低于
`5 ms／100 ms` 参考线；尖峰发生后真实观察数和完整输出继续增长，最终状态 ready，未出现 Hook
中断或输出失败。因此保留为发布后 G-POST 性能观察项，不调高参考线，也不作为本轮阻断缺陷。

## 6. 结论与限制

G-PRE **通过**。自动门槛、当前环境可执行的 PRE-01～09、关键输入／保存／恢复和受影响退出边界
无发布阻断缺陷，可以进入 H。以下限制没有被隐藏：

- 睡眠／唤醒、提升完整性目标的真实 UIPI 拒绝、partial `SendInput`、多屏／跨 DPI、特殊键硬件
  未在本轮新增物理证据；确定性覆盖及既有边界仍保留。
- 强制结束只用于已核验的隔离测试 PID；该路径不能恢复已经暂扣的历史物理输入。
- 本记录不是 24／72 小时稳定性证明；长测继续留在首版发布后的 G-POST。
