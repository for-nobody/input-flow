# InputFlow Phase F、首版发布与发布后长测执行记录

> 创建日期：2026-10-02（Australia/Brisbane）  
> 更新日期：2026-10-08（Australia/Brisbane）
> 当前状态：Phase F 的 F0～F3 代码与自动验证已完成；F4 真实 Windows 输入／短时资源验收和 F5 现场收口未执行。未进入 G-PRE／H／RC，未发布 release。
> 证据要求：自动、故障注入、脚本注入、真实物理输入、用户观察、继承历史基线分开记录。

## 1. 用户确定的范围

- 首个 release 包含鼠标四方向，完成 Phase F 后才发布。
- G-PRE 的短时回归／恢复和 H 的交付验收在发布前。
- G-POST 24／72 小时长测和长期 daily-drive 在首版发布后，不阻塞首版。
- 保留 Rust／Win32 Agent＋WinUI 3 设置架构和 M6 不变量。

## 2. 开工实际基线（Codex 填写）

| 字段 | 实际值 |
|---|---|
| 日期／时区 | 2026-10-08，Australia/Brisbane |
| 分支／HEAD／工作区改动 | `main`，开工 HEAD `edd6187ea02165de15bd6ed69e5e7f8210f6ede3`，开工工作树 clean；本记录对应未提交 Phase F 工作树 |
| 适用 AGENTS.md | 仓库内未发现 `AGENTS.md` |
| Windows build／架构／会话 | Windows 10 Pro 25H2，内核 10.0.26200.9457，x64；本轮未启动物理验收目标窗口 |
| Rust／.NET／Windows SDK／App SDK | Rust／Cargo 1.98.1；.NET SDK 10.0.401／MSBuild 18.9.11；Windows SDK 10.0.26100.0；Windows App SDK 2.5.1；VS Build Tools 2026 18.10.2 |
| 鼠标／键盘／显示器／DPI／布局 | 本轮未重新盘点硬件且未执行 F-PHY；不得继承 Phase E 的 200%／布局／设备观察为本轮结果 |
| 新执行自动基线与证据路径 | 见本记录第 5.1 节；最终 Rust 169／169、C# 17／17，Debug／Release／联合构建通过 |

历史参考：Phase E 最终记录 Rust 151／151、protocol 6／6、Settings Core 11／11，五分钟资源和 M6 实机矩阵已收口。使用时引用 `M7-Phase-E与M6-Windows实机验收记录.md`，不要将其复制成自己的新执行结果。

## 3. 总进度

| 阶段 | 初始状态 | 证据／阻塞 |
|---|---|---|
| F0 ADR-008 | 完成 | `docs/decisions/ADR-008-鼠标方向规则与直通策略.md` |
| F1 算法／状态机 | 完成 | 固定四槽方向组、净位移、owner 时间边界、一次命中、cancel／repeat／overflow／tombstone 自动测试 |
| F2 Hook／runtime | 完成（自动） | GetCursorPos 起点、move／wheel 规范化、无规则／旁路快速直通、锁外输出平台测试；物理 Hook 观察留 F4 |
| F3 配置／IPC／WinUI | 完成（代码／自动） | Schema v4、v1～v3 迁移、`config_v4`、跨语言 fixture、编辑器和有限预览；真实 UI 保存／重开留 F-PHY-01 |
| F4 实机／F5 回归文档 | 进行中 | 自动回归和文档已完成；F-PHY-01～07、受影响现场回归未执行 |
| G-PRE 有限可靠性 | 未开始 | F 完成后 |
| H 分发与生命周期 | 未开始 | F／G-PRE 完成后；设计可提前准备 |
| RC 最终包与发布草稿 | 未开始 | 最终分发可验收后 |
| 首个远端 Release | 未发布 | 实际发布授权及结果待记录 |
| G-POST 长测 | 计划于首版发布后执行 | 不阻塞首版 |

## 4. 每个开发阶段使用的记录模板

### 阶段／编号：待填写

- 起止时间、基线 HEAD、工作区状态：
- 具体行为／问题和最小复现：
- 原因假设及排除依据：
- 候选方案、最终选择与取舍：
- 对既有不变量和配置兼容的影响：
- 修改文件：
- 自动命令／退出码／测试数量／输出路径：
- 故障注入／脚本输入证据：
- 真实物理手势、目标应用、预期／实际、用户观察：
- 修复前失败证据与修复后新证据：
- 未执行／环境限制：
- 结论：未开始／进行中／完成／环境受限／阻断。
- 下一项最小任务：

记录是完整结论，原始输出可保存在会话证据目录；不要只写“已修复、全部通过”。

## 5. Phase F 设计与验收

| 决策 | 最终值／ADR／测试 |
|---|---|
| 激活键 Down／repeat／Up 归属 | ADR-008：候选 Down／repeat 暂扣；失败连同 Up FIFO 回放，命中消费并保留真实 Up tombstone；move 永远直通 |
| 单次按住、失败后重新武装规则 | 命中最多一次；失败／取消后 repeat 不重启，必须真实 Up 后的新非 repeat Down 才重新武装 |
| 坐标起点／单位／时间源 | activation Down 的 `GetCursorPos` 屏幕像素；move 用 Hook `pt`；i64 净位移；matcher 单调时钟 |
| 阈值／偏轴／超时及默认值 | 80 px／500 ms／40 px；合法范围 10～2000／100～5000／0～2000；距离和容差等于边界均满足 |
| 四方向组／斜线／跨类型冲突 | 同 identity 固定四槽且参数相同；主轴胜出、等轴不命中；同方向重复和跨既有 trigger 前缀拒绝 |
| 注入 move／坐标跳变 | 自身 injection 在平台快速直通；第三方 injected move 取消候选并回放；无 injected 标记的系统跳变是明确限制 |
| Schema／迁移／能力门槛 | 严格 Schema v4，继续读取 v1／v2／v3；wire v1 不变；handshake 要求 schema 4 与 `config_v4` |
| 预览取消／节流／隐私 | UI 显式开始／取消，33 ms DispatcherTimer，配置时间窗到期；仅保存起点／当前点和摘要，不执行动作或保存轨迹 |

| 场景 | 自动结果 | 真实物理结果 | 证据路径／限制 |
|---|---|---|---|
| 四方向阈值／一次性 | 通过：四方向等阈值、命中一次、真实 Up 模型后重武装 | 未执行 | `inputflow-engine` matcher tests |
| 抖动／偏轴／超时／提前释放 | 通过：净位移折返、等轴、容差内／外、deadline owner 顺序、失败回放 | 未执行 | matcher 自动序列；真实手感留 F-PHY-03 |
| 四方向 UI／保存／重开 | v4 fixture 和 C# typed round-trip 通过；WinUI Debug／Release 构建通过 | 未执行 | F-PHY-01 仍需页面实际保存／重开 |
| pause／replace／capture／quit | pause／replace pending flush 和命中 tombstone 自动通过；既有 capture／quit 回归通过 | 未执行 | 现场交错留 F-PHY-04／07 |
| repeat／队列满／tombstone | 通过：有界 FIFO 原序回放、当前 repeat 直通并旁路、命中 Up 消费 | 未执行 | matcher 自动序列 |
| 负坐标／跨屏 | i32 极值差值／跨原点自动通过 | 未执行 | 自动结果不是多屏／跨 DPI 物理证明 |
| 125／500／1000 Hz 压力 | 通过：每组一次命中、无 bypass、move 不增长 pending | 未执行 | 只是合成模型，不是实际设备频率 |
| 五分钟物理移动资源 | 不适用 | 未执行 | 样本数／分位／CPU／内存／句柄待记录 |
| M6／Phase E 受影响回归 | Rust workspace、C# contract／Core、WinUI Debug／Release 和联合构建通过 | 未执行 | 历史物理基线不自动升级；受影响现场路径留 F4／F5 |

### 5.1 本轮实现与自动命令

- 引擎：新增 `MouseKind::Move`、`MouseDirection`、固定四槽 `MouseDirectionGroup` 和 activation cursor snapshot；move 只做固定大小净位移计算，始终返回 `PassThrough`。测试覆盖四方向、边界、折返、一次性、cancel、deadline、repeat／overflow、pause／replace、physical-first、i32 极值、注入和 125／500／1000 Hz 序列。
- Windows：方向规则存在时才规范化 move／wheel；旁路／无方向规则快速 `CallNextHookEx`。activation 非 repeat Down 获取 `GetCursorPos`；自身注入直接跳过 matcher，第三方 injected move 取消候选。平台测试确认方向命中仍让 move 直通并在释放 matcher 锁后 dispatch。
- 配置／IPC：Schema v4 严格 DTO、范围／组冲突校验、v1～v3 确定迁移和 v4 golden；agent／C# 同时报 `config_v4`，wire v1 保持不变。
- UI：增加方向、距离、时间窗、偏轴编辑与有限本地预览；预览显式开始／取消／超时，只显示摘要且不执行动作。
- `cargo fmt --all -- --check`：通过。
- `cargo test --workspace`：169／169，通过（agent 4、config 32、engine 87、protocol 12、runtime 6、windows 28）。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- C# protocol contract：6／6；Settings Core：11／11。
- WinUI solution Debug／Release：均 0 warning、0 error；`scripts/build-windows.ps1 -SkipRestore`：通过。
- 第一次直接 Rust 命令因普通 shell 缺 MSVC CRT library 失败，改由已安装 Build Tools 2026 `VsDevCmd.bat` 后通过；这是环境初始化，不是源代码失败。
- Release 首次 `--no-restore` 因默认 restore 未取得 Release 条件启用的 runtime／ILLink packs 而失败；按 Release 配置 restore 后通过，并修正联合脚本为 Debug／Release 分别 restore，避免干净缓存复现该问题。
- 没有用自动脚本注入充当物理输入；本节没有 stats RTT、callback 分位、CPU／内存／线程／句柄的新物理负载数字。

## 6. G-PRE 与 H／RC

| 编号组 | 状态 | 实际操作／结果／证据 |
|---|---|---|
| PRE-01～09 | 未执行 | 待记录 |
| DIST-01～07 | 未执行 | 待记录 |
| RC 固定提交／版本同步 | 未执行 | 待记录 |
| RC 最终包 smoke | 未执行 | 待记录 |

### 最终产物（实际生成后填写）

- 发布版本／tag／精确提交／是否含未提交输入：
- Agent／UI 版本及元数据：
- 发布模式／支持 OS／架构／依赖：
- 构建与打包命令、工具链、退出结果：
- 包名、位置、大小、SHA-256：
- 干净环境与测试范围：
- 自启动／升级／移除的实测证据：
- 许可证／NOTICE／签名状态：
- 发布标题／正文／附件：
- 对应授权、实际远端动作、release URL／发布时间：
- 结论：开发完成／发布准备完成／已发布，必须区分。

## 7. 发布后 G-POST（首版发布后再填写）

| 会话 | 实际包／校验和 | 墙钟／进程存活／采样覆盖 | 睡眠／重启／输入时段 | 资源／错误结论 |
|---|---|---|---|---|
| POST-A | 待记录 | 未执行 | 待记录 | 待记录 |
| POST-B 24h | 待记录 | 未执行 | 待记录 | 待记录 |
| POST-C 72h | 待记录 | 未执行 | 待记录 | 待记录 |

重启或升级后另建会话。补丁版本不回写旧版本为通过；记录失败复现、修复提交和新包验证。

## 8. 已知限制和下一步

继承基线的 keypad Enter／独立播放键、Narrator、partial SendInput 和部署限制；新增未验证范围为真实四方向／拖拽、物理 polling rate、多屏／跨 DPI、第三方无 injected 标记的光标跳变和五分钟方向资源样本。长测当前状态固定为“首版发布后执行”，直到真实结果产生。

下一项最小任务：使用隔离的 `scripts/acceptance/configs/mouse-direction-f8-four.json` 执行 F-PHY-01～07，并把真实目标应用、输入序列、日志和资源口径回填本记录；通过后完成 F5，再进入 G-PRE。此处没有等待 24／72 小时的发布前条件。
