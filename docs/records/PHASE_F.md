# InputFlow Phase F 执行记录

> 文档类型：当前阶段证据记录
>
> 阶段：Phase F／M8 鼠标方向
>
> 状态来源：[`../status/CURRENT_STATUS.md`](../status/CURRENT_STATUS.md)
>
> 更新日期：2026-10-08（Australia/Brisbane）

本文只记录 Phase F 的实际设计、实现、自动验证和现场证据。G-PRE、H、RC 和 G-POST 不在此处
预建模板；进入相应阶段时另建单阶段记录。

## 1. 实现基线

| 字段 | 实际值 |
|---|---|
| 开工提交 | `edd6187ea02165de15bd6ed69e5e7f8210f6ede3` |
| Phase F 实现提交 | `3c3a158821614419ec6b5b4b000f94c8d9f6cbdf` |
| 分支 | `main` |
| 适用 AGENTS.md | 实现时仓库内未发现 `AGENTS.md` |
| Windows／架构 | Windows 10 Pro 25H2，内核 10.0.26200.9457，x64 |
| Rust／.NET | Rust/Cargo 1.98.1；.NET SDK 10.0.401；MSBuild 18.9.11 |
| Windows 工具链 | Windows SDK 10.0.26100.0；Windows App SDK 2.5.1；VS Build Tools 2026 18.10.2 |
| 物理环境 | 尚未为 F-PHY 重新盘点；不得继承 Phase E 的 DPI／设备观察作为本阶段结果 |

Phase E 的 Rust 151／151、C# 17／17、五分钟资源和 M6 矩阵属于历史基线，见
[`../archive/m7/M7_PHASE_E_AND_M6_ACCEPTANCE.md`](../archive/m7/M7_PHASE_E_AND_M6_ACCEPTANCE.md)。

## 2. 固定设计

| 决策 | Phase F 结果 |
|---|---|
| 激活键归属 | 候选 Down／repeat 暂扣；失败连同 Up FIFO 回放；命中消费并保留真实 Up tombstone；move 始终直通 |
| 重新武装 | 一次按住最多命中一次；失败／取消后 repeat 不重启，等待真实 Up 后的新非 repeat Down |
| 坐标／时间 | activation Down 使用 `GetCursorPos` 屏幕像素；move 使用 Hook `pt`；i64 净位移；matcher 单调时钟 |
| 默认参数 | 80 px／500 ms／40 px；合法范围 10～2000／100～5000／0～2000；等于边界满足 |
| 方向组 | 同 identity 固定四槽且参数一致；主轴胜出、等轴不命中；重复方向和跨 trigger 前缀拒绝 |
| 注入与跳变 | 自身 injection 快速直通；第三方 injected move 取消候选并回放；无标记跳变保留为限制 |
| Schema／协议 | 严格 Schema v4，读取 v1～v3；wire v1 不变；handshake 要求 schema 4 与 `config_v4` |
| WinUI 预览 | 显式开始／取消，33 ms 节流，配置时间窗到期；仅固定摘要，不执行动作或保存轨迹 |

完整设计依据是
[`../decisions/ADR-008-鼠标方向规则与直通策略.md`](../decisions/ADR-008-鼠标方向规则与直通策略.md)。

## 3. 已实现内容

- 引擎新增 `MouseKind::Move`、`MouseDirection`、固定四槽 `MouseDirectionGroup` 和 activation cursor
  snapshot。move 只做固定大小净位移计算并始终返回 `PassThrough`。
- matcher 覆盖四方向、边界、折返、一次命中、cancel、deadline、repeat／overflow、pause／replace、
  release tombstone、physical-first、i32 极值、注入和 125／500／1000 Hz 合成序列。
- Windows 层只在存在方向规则时规范化 move／wheel；旁路或无方向规则时快速 `CallNextHookEx`。
  activation 的非 repeat Down 获取 `GetCursorPos`；自身注入跳过 matcher，第三方 injected move 取消候选。
- 平台测试确认方向命中仍让 move 直通，并在释放 matcher 锁后 dispatch 动作。
- 配置加入严格 Schema v4、范围／组冲突校验、v1～v3 迁移和 v4 golden；Rust Agent 与 C#
  client 同时报 `config_v4`，wire v1 保持不变。
- WinUI 增加方向、距离、时间窗、偏轴编辑和有限本地预览；预览不执行动作。
- 联合构建脚本对 Debug／Release 分别 restore，避免干净缓存缺少 Release runtime／ILLink packs。

## 4. 自动验证

| 命令／范围 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | 通过 |
| `cargo test --workspace` | 169／169：agent 4、config 32、engine 87、protocol 12、runtime 6、windows 28 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| C# protocol contract | 6／6，通过 |
| C# Settings Core | 11／11，通过 |
| WinUI solution Debug／Release | 均 0 warning、0 error |
| `scripts/build-windows.ps1 -SkipRestore` | 通过 |

第一次从未初始化的普通 shell 直接运行 Rust 命令时，因缺少 MSVC CRT library 失败；通过已安装的
Build Tools 2026 `VsDevCmd.bat` 初始化环境后通过。这是环境初始化问题，不是源代码失败。

Release 首次 `--no-restore` 因默认 restore 未取得 Release 条件启用的 runtime／ILLink packs 而失败；
按 Release 配置 restore 后通过，并已修正联合脚本。

合成 125／500／1000 Hz 只证明确定性有界行为，不是实际设备 polling rate、Hook 存活或真实输入证据。

## 5. 现场验收记录

执行步骤见 [`../tasks/PHASE_F_ACCEPTANCE.md`](../tasks/PHASE_F_ACCEPTANCE.md)，工具说明见
[`../guides/WINDOWS_ACCEPTANCE.md`](../guides/WINDOWS_ACCEPTANCE.md)。

| 编号 | 状态 | 环境／操作／结果／证据 |
|---|---|---|
| F-PHY-01 | 未执行 | UI 四方向组保存与重开待记录 |
| F-PHY-02 | 未执行 | 真实四方向、一次命中与重新武装待记录 |
| F-PHY-03 | 未执行 | 抖动、距离、提前释放、超时和斜线待记录 |
| F-PHY-04 | 未执行 | F12／pause／replace／预览取消待记录 |
| F-PHY-05 | 未执行 | 约五分钟物理 move 和资源样本待记录 |
| F-PHY-06 | 未执行 | 普通输入、菜单和拖拽待记录 |
| F-PHY-07 | 未执行 | 按住激活键暂停及正常退出待记录 |

现场执行时，为每一项补充提交／产物、目标应用、物理输入序列、预期、实际、用户观察、日志目录、
资源口径和通过／失败／环境受限结论。失败修复必须保留修复前证据和修复后新结果。

## 6. 当前限制

- 本阶段没有新的物理 stats RTT、callback 分位、CPU、内存、线程或句柄结果。
- 真实四方向、普通拖拽、物理 polling rate、多屏／跨 DPI 和第三方无 injected 标记的跳变未验证。
- 自动测试和脚本注入不能填入 F-PHY 的真实物理结果栏。
- Phase F 只有在 F-PHY-01～07 和受影响现场回归收口后才能标记完成。
