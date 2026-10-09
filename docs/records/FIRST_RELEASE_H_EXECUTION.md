# 首版发布 H 执行记录

> 文档类型：当前阶段事实记录
>
> 状态：本机可执行范围完成；等待独立干净环境
>
> 开始时间：2026-10-09（Australia/Brisbane）
>
> 任务卡：[`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md)

## 1. 进入条件与基线

- G-PRE 已通过，完整证据归档于 [`../archive/first-release/G_PRE.md`](../archive/first-release/G_PRE.md)。
- H 从 `main` 提交 `001e4ad8dbf1137ed114425e1953a7dceb7469c0` 进入；后续实现与证据由本记录
  列出并随仓库提交。H 工程包 manifest 保留其实际构建时的提交和 dirty 状态，不能把它误写为 RC。
- 用户已有的 `docs/planning/InputFlow-Touchpad-Expansion-Proposal.md` 是未来提案，不纳入首版分发输入。
- 当前只进入 H；RC、远端 tag／release 和 G-POST 尚未开始。

## 2. 问题定义

开发构建已经可运行，但尚无一个普通 Windows x64 用户可直接解压使用、明确依赖、从任意路径启动、
可选登录自启动、可升级和可移除的固定目录包。还需要证明 Agent 能从包内定位 Settings，配置继续写入
用户数据目录，并明确无法在本机冒充的干净环境证据。

## 3. 最小方案

1. 完成 ADR-009，比较 unpackaged、self-contained、framework-dependent 和可选用户级安装器；默认先
   验证 x64 便携目录包与 UI 的 .NET／Windows App SDK 双 self-contained。
2. 新增统一打包入口：先执行完整构建门槛，再发布 UI、复制 Agent／用户文件、检查依赖和布局、生成
   zip 与 SHA-256；任一步失败返回非零。
3. 采用同目录 Agent + Settings 布局；从空格／中文路径、非程序工作目录、托盘、重复启动验证定位。
4. 为当前用户 Startup 快捷方式提供透明、幂等、可移除的 enable／disable／status 入口，默认关闭，
   只启动 Agent。
5. 写明并验证便携升级／移除流程、Schema 降级边界和默认保留 `%LOCALAPPDATA%\InputFlow`。
6. 在本机完成可执行的 DIST 矩阵；拿不到无 SDK／runtime 的独立环境时，将 DIST-01／02／04 的相应
   干净环境或重登录部分保持未完成，不用开发机结果冒充。

## 4. 当前状态

H0～H4 的实现、本机可执行矩阵和真实重启登录验证已完成。当前存在一个可审阅的 H 工程包，但它
来自未提交工作树，不是 RC／最终 release。H5 所需独立干净环境尚未取得，因此不能声明“分发已
验收”，也不进入 RC。

## 5. 环境与完整门槛

本轮实际环境：Windows 注册表 `Windows 10 Pro 26H2`、build `26300.9457`、x64；rustc／cargo
`1.99.0`、.NET SDK `10.0.401`、MSBuild `18.9.11.42413`。系统产品名与数值 build 可能不一致，证据
以数值 build 为主。`scripts/build-windows.ps1 -SkipRestore` 已从 `vswhere` 返回的候选中选择实际包含
x64 import libraries 的 `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools`，避免 PATH 中
不完整的 Community 安装造成 `msvcrt.lib` 链接失败。

未来 Schema 降级保护改动后的完整结果：

- Rust workspace **172／172**：agent 4、config 33、engine 87、protocol 12、runtime 6、windows 30；
  `cargo fmt --check`、Clippy `-D warnings`、probe 和 Release Agent 构建通过。
- WinUI Debug／Release 均 0 warning、0 error。
- C# protocol contract 6／6；Settings Core **12／12**，新增未来 Schema 明确拒绝检查。
- 统一打包入口在此前已实际执行“完整门槛 → publish → 组包 → zip → SHA-256”并成功；已有输出且
  未传 `-Force` 时在构建前非零拒绝覆盖。最后一次源码完整门槛通过后，使用
  `-SkipBuild -SkipRestore -Force` 重组同一工作树最终 H 工程包。

一次绕过统一入口直接运行 Cargo 定向测试时，Rust 选择了缺失 import libraries 的 VS Community
linker 并报 `LNK1104 msvcrt.lib`；这不是测试失败结论。统一入口现会初始化完整 MSVC 环境，并在该
环境中使相同测试及全 workspace 通过。

## 6. H0／H1：方案、发布与依赖

- 分发决定见 [`../decisions/ADR-009-首版分发与用户生命周期.md`](../decisions/ADR-009-首版分发与用户生命周期.md)：
  unpackaged x64 便携目录，Agent 与 Settings 同目录，Settings 的 .NET 与 Windows App SDK 双
  self-contained；不使用 trim、single-file、AOT 或 ReadyToRun。
- 发布 profile：`apps/settings-winui/InputFlow.Settings/Properties/PublishProfiles/win-x64.pubxml`。
- 统一入口：`scripts/package-release.ps1`。包名、Agent／Settings 版本、PE x64、必要文件和禁止文件
  在压缩前校验；输出根必须在仓库内，替换已有输出必须显式 `-Force`。
- 首次 `dotnet publish` 的 Settings 可启动但约 10 秒后以 `0xc000027b` 崩溃；对比发现 publish
  遗漏项目 `InputFlow.Settings.pri`。项目增加存在性保护的 publish item target 后，PRI 随包输出，
  Settings 已稳定运行。该问题与 Microsoft Windows App SDK issue #6720 描述一致。
- Agent PE imports 复核仍为 Windows 系统 API、Universal CRT API set 和 `VCRUNTIME140.dll`；包内不
  偷拷系统 DLL。目标机需要中央安装 Microsoft Visual C++ Redistributable 2015–2022 x64。本机
  `VCRUNTIME140.dll` 为 `14.51.36247.0`；缺少该依赖的真实目标环境尚未取得。

最终 H 工程包（2026-10-09 21:59 +10:00）：

| 项目 | 实际值 |
|---|---|
| 目录 | `target/distribution/InputFlow-0.1.0-win-x64` |
| zip | `target/distribution/InputFlow-0.1.0-win-x64.zip` |
| SHA-256 | `D20D04525E2161F66EBBD01EA2BC67E63841523D335C670CB8571AC6C32EFA1B` |
| 目录体积 | 243,409,590 bytes |
| zip 体积 | 93,838,781 bytes |
| 文件数 | 526（manifest 写入前 525） |
| `InputFlow.Settings.pri` | 2,201,344 bytes |
| Agent SHA-256 | `88A8F32C4C30A966FAF98D12F94568C661F66D8F68DBA48F8F251836AEE4F51A` |
| 禁止内容 | PDB 0、`config.json` 0、`agent.log` 0 |

`SHA256SUMS.txt` 已回读并与 zip 实算值一致。`package-manifest.json` 如实记录提交
`001e4ad8dbf1137ed114425e1953a7dceb7469c0`、`worktree_dirty=true`、双 self-contained、未裁剪及
外部 VC++ x64 依赖；因此本包只用于 H 验证，不冒充固定 RC。

## 7. H2：布局与启动

- 一个早期 H 包从 `target/acceptance/h/安装 路径 2/InputFlow 中文/...`、非程序工作目录启动；Agent
  托盘隐藏窗口存在，模拟托盘双击从自身目录打开 Settings。Agent／Settings 第二实例均退出 0，
  Settings 正常 `WM_CLOSE` 退出，Agent 有界退出 0，无 Application Error 或残留进程。此后源码只
  修改了未来 Schema 写保护，不影响路径、托盘或进程定位。
- 最终工程包在 `target/acceptance/h/final-package-basic/foreign cwd 中文` 再次联合启动：Agent
  `phase=ready`、规则 0、未暂停；Settings 稳定 10 秒后正常退出 0，Agent 有界退出 0，残留进程 0。
  首次启动没有创建配置，只在隔离 `%LOCALAPPDATA%` 创建日志。
- 最终 zip 之前的精确包补充验证曾从空格／中文路径稳定运行 Settings 12 秒并通过 Agent／Settings
  单实例。非交互工具宿主中托盘 API 会返回 `Shell_NotifyIconW(NIM_ADD)` 失败，因此最终包的自动
  补充运行使用 `--no-tray`；真实桌面托盘结论只采用前述真实桌面结果，不把宿主限制写成产品通过。

## 8. H3：当前用户自启动

实现 `Manage-InputFlow-Autostart.ps1` 与 enable／disable／status 三个 CMD。默认关闭，只管理当前用户
Startup 中的 `InputFlow Agent.lnk`，目标为 Agent、参数为空、工作目录为包目录；同名但目标不是
`inputflow-agent.exe` 的入口会被拒绝且保留。

验证结果：

- 隔离 Startup：空格／中文路径、重复 enable、旧路径 `enabled_other_path`、移动后刷新、disable、
  以及拒绝并保留指向 `cmd.exe` 的同名外来快捷方式均通过。
- 包内 CMD 最初暴露 Windows PowerShell 5 的参数绑定问题：`-File` 模式在默认参数表达式中使用
  `$PSScriptRoot` 会得到空值。默认 Agent 路径移入脚本体解析后，从非程序中文工作目录实际调用三个
  CMD，退出码均为 0；系统 Startup 状态为 `enabled_current_path`，目标／工作目录正确，随后 disable
  恢复为 `disabled` 且快捷方式已删除。
- 2026-10-10 00:29（Australia/Brisbane）用户手动重启并重新登录后，在没有手动启动 InputFlow 的
  前提下只存在 1 个 Agent，路径为最终 H 工程包，Settings 进程数为 0。Agent IPC 于 00:40 回读为
  `phase=ready`、规则 0、未暂停、`last_error=null`；日志显示 Hook 与 IPC 就绪、
  `previous_abnormal=false`，且没有 `Shell_NotifyIconW` 错误，证明登录托盘启动链成功。
- 重启后 Startup 仍为 `enabled_current_path`，目标和工作目录保持最终包路径。00:42 通过包内 disable
  CMD 恢复 `disabled`，快捷方式确认删除；已经运行的 Agent 保持 1 个、Settings 保持 0，符合“禁用
  只影响下次登录，不强制结束当前 Agent”的语义。

2026-10-10 00:27（Australia/Brisbane），用户确认可以手动重启并在 IDE 中恢复本会话。重启前已再次
核对 zip SHA-256 为 `D20D04525E2161F66EBBD01EA2BC67E63841523D335C670CB8571AC6C32EFA1B`、
InputFlow 进程数为 0，并将真实 Startup 入口置为 `enabled_current_path`；目标和工作目录均为上述最终
H 工程包目录。该入口已经按上一条结果完成真实重启验证并恢复禁用。

## 9. H4：升级、降级与移除

便携生命周期采用两个完整包目录和隔离用户数据模拟；这是**同源码目录切换模拟，不是真实版本／
Schema 二进制升级**。旧／新 Agent SHA-256 不同是重新链接所致。两边均正常退出 0、均读取 3 条
启用规则，禁用规则保持禁用；配置 SHA-256 前后均为
`768E313051C046B21B44C2586FB9D09E9F1917571B5BEF9CF23B19A86DBCFF31`。自启动从旧路径识别为
`enabled_other_path` 后更新到新路径并禁用。两个完整程序目录被删除、入口被删除、用户配置保留，
产品残留进程为 0。

未知新 Schema 的首次测试发现旧 Agent 会旁路到空配置但仍可能接受后续保存，不满足任务卡。修复后：

- 配置层检测主文件或高优先级恢复文件的未来 Schema，使用无规则旁路但设置写保护；不会回退旧备份。
- Runtime status 的 `last_error` 明确报告“本构建只支持到 v4，写入被阻止以避免降级数据丢失”。
- 对最终包的 v5 文件实际发送 v4 `apply_config`，结果为 `persistence_failed`、
  `recovery_required=false`；v5 文件 SHA-256 前后均为
  `0EB3398F6BA26D54B30C61447AAE67343A3B9FAA1FE14DB1C3981F06FFE9A28F`，备份／临时文件为 0，
  Agent 正常退出 0。
- Settings Core 同时明确拒绝高于 v4 的文档。用户指南说明必须改用支持该 Schema 的版本。

## 10. H5／DIST 矩阵

| 编号 | 当前结果 | 结论 |
|---|---|---|
| DIST-01 | 开发机包内 Agent／托盘／UI 与双 self-contained 组件已运行；机器装有 SDK、仓库和 VC++ runtime | **部分；缺独立干净机** |
| DIST-02 | 同提交产品路径已有四方向保存／读回／真实命中；最终包 UI 可运行，但未在干净机重做 | **部分** |
| DIST-03 | 空格／中文路径、任意工作目录、单实例、真实桌面托盘 Open Settings 通过 | **本机通过** |
| DIST-04 | 隔离矩阵、真实 Startup CMD 及一次用户手动重启通过；登录后仅一个 Agent、Settings 未常驻，随后恢复禁用 | **本机通过；待干净机复核** |
| DIST-05 | 配置／禁用状态保留模拟和最终包未来 Schema 写保护通过；未冒充真实版本升级 | **本机范围通过** |
| DIST-06 | 正常有界退出、禁用入口、删除完整程序目录、保留数据、无残留进程通过 | **本机通过** |
| DIST-07 | x64、VC++ 依赖、未签名提示和不得关闭系统保护已写明；缺依赖／错误架构目标机未取得 | **部分** |

本机未发现 Windows Sandbox、Hyper-V `Get-VM`／服务、Docker、VirtualBox 或 VMware。查询 Windows
optional feature 还需要提升，但对应 Sandbox 可执行文件不存在；因此没有可诚实充当“无 Rust、Visual
Studio、.NET SDK 或开发仓库”的第二个 Windows 环境。

## 11. 未完成项与进入 RC 条件

H 当前只差独立干净环境现场证据：在新 VM／第二台 Windows x64 设备完成 DIST-01～07 的缺口，尤其
首次解压、四方向真实命中、一次干净用户登录启动、缺少 VC++ 依赖时的可理解失败以及正确安装依赖
后的恢复。当前工作机器本身虽是 VM，但已经安装 Rust、Visual Studio、.NET SDK 并包含开发仓库，
不能充当该独立干净环境。

取得并记录这些结果前，`FIRST_RELEASE.md` 的 H 门槛保持未勾选，不开始 RC，不把当前工程包称为
最终发行包。
