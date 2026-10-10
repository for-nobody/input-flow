# 首版发布 RC 执行记录

> 文档类型：当前阶段事实记录
>
> 状态：完成（固定包已通过 RC；远端未发布）
>
> 开始时间：2026-10-10（Australia/Brisbane）
>
> 任务卡：[`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md)

## 1. 进入条件与用户决定

- H 已按声明支持范围完成，基线提交为 `adf2f3e75a45fee54856a3691a21e45a798f0fd3`；H 工程包只作继承
  证据，不能改名为 RC 包。
- 用户确定首版版本为 `v0.9.0`，发布平台仍按公开测试版标记 Pre-release。
- 用户确定项目许可证为 MIT；仓库许可证版权身份采用当前仓库所有者与提交身份 `for-nobody`。
- 没有远端 tag／release 授权；RC 只准备本地固定产物、校验和与可审阅发布草稿。

## 2. RC 问题与最小方案

H 代码和工程包仍显示 `0.1.0`，关于页还保留 Phase A 的 framework-dependent 说明，发布包也未附项目
许可证和各锁定第三方组件的完整许可文件。RC 必须先统一版本与说明，增强统一打包入口对干净工作树、
版本和法律附件的检查，再固定提交生成精确包。

最小方案：

1. 将 Rust workspace、Agent handshake、Settings assembly/file/informational version、协议客户端、
   fixtures、Windows manifests、包名和关于页统一为 `0.9.0`／`0.9.0.0`。
2. 增加 MIT `LICENSE`、第三方组件清单和 `v0.9.0` 发布说明；打包时从锁定 Cargo／NuGet／.NET 输入
   复制实际法律文件。
3. 为 `scripts/package-release.ps1` 增加 `-RequireClean`，在构建前后拒绝未提交发布输入，并验证所有
   版本面一致。
4. 完整门槛通过后固定本地提交，从该干净提交生成 `InputFlow-0.9.0-win-x64.zip` 与
   `SHA256SUMS.txt`，再只对该包做 RC smoke。

## 3. 执行证据

### 3.1 固定提交与阻断修复

- 版本、许可证与发布材料首先固定为 `6558bfb4c6a7c9da903c6fc5d9f1c447c463ae1a`。
- 第一次候选包通过构建、IPC 和桌面启动检查，但进一步解压审计发现三个自研二进制仍含构建用户绝对
  路径：两个 .NET 项目引用 DLL 保留 CodeView/PDB 记录，Rust Agent 保留标准库与 Cargo 源位置。
  该包及其 SHA-256 已作废，不能发布。
- 修复方案是为 Release WinUI 解决方案统一关闭调试目录、为 Release Agent 重映射工作区／用户路径，
  并把二进制路径扫描固化为打包失败条件。最终包构建提交为
  `bc48c36d73b94106d53fe192175dc005f756fe25`；构建前后工作树均干净。

### 3.2 工具链与完整门槛

本机为 Windows NT `10.0.26300.0` x64；统一入口实际选择 Visual Studio Build Tools 2026 `18.10.2`
与 MSVC `14.51.36231`。其余工具为 `rustc 1.99.0`、`cargo 1.99.0`、`.NET SDK 10.0.401`、
Windows PowerShell `5.1.26100.9444`。

从固定提交执行：

```powershell
.\scripts\package-release.ps1 -RequireClean -Force
```

命令退出码为 0，结果如下：

- Rust workspace **172／172**：Agent 4、config 33、engine 87、protocol 12、runtime 6、windows 30；
  `cargo fmt --check`、严格 Clippy、probe 和带私有路径重映射的 Release Agent 构建通过。
- WinUI solution Debug／Release 均为 **0 warning、0 error**。
- C# protocol contract **7／7**；Settings Core **12／12**。
- Settings publish 使用 .NET 与 Windows App SDK self-contained、x64、未裁剪、非 single-file、非 AOT、
  非 ReadyToRun；外部前置仍只有已声明的 VC++ Redistributable 2015–2022 x64。

### 3.3 最终包与静态回读

| 项目 | 最终值 |
|---|---|
| 目录／zip | `InputFlow-0.9.0-win-x64`／`InputFlow-0.9.0-win-x64.zip` |
| 构建提交 | `bc48c36d73b94106d53fe192175dc005f756fe25` |
| SHA-256 | `A287489C5224DE45685B2B89D799C44472384CF22B12C98E13B3C318BC004537` |
| 解压大小／zip 大小 | 245,226,071／94,128,454 bytes |
| 文件数 | 579（manifest 记录写入前 578） |
| Settings FileVersion／ProductVersion | `0.9.0.0`／`0.9.0+bc48c36d73b94106d53fe192175dc005f756fe25` |
| 项目 PRI | 2,201,608 bytes |

`package-manifest.json` 回读为 `version=0.9.0`、`release_tag=v0.9.0`、
`release_channel=pre-release`、`project_license=MIT`、`worktree_dirty=false`、
`build_paths_redacted=true`。`SHA256SUMS.txt` 与重新计算的 zip 哈希一致。包内有 50 个 `Licenses/`
法律文件以及项目 LICENSE／NOTICE；PDB、正式配置、日志、running marker、仓库绝对路径和构建用户绝对
路径的命中数均为 0。

### 3.4 精确 ZIP 自动与桌面壳层 smoke

所有下列运行均来自最终 zip 的全新解压副本，而不是 Debug 输出或打包前目录：

- 解压得到 579 个文件；Agent 使用四方向配置执行 `--smoke-iterations 100`，退出码 0，确认 4 条规则，
  `cleanup_warnings=0`，最终 `output_failed=0`、`output_dropped=0`、hook/logger panic 均为 false。
- C# live contract 客户端退出码 0；7 项 fixture 契约以及真实 Agent handshake、配置读取／验证／应用、
  pause／resume、stats、capture cancel 和捕获客户端断连清理全部通过。Agent 写出干净停止记录。
- 最终包 Agent IPC 就绪后，真实 `InputFlow 设置`窗口成功建立；正常关闭请求返回 true，Settings 退出码
  为 0，Agent 随后干净停止，健康字段无失败。

自动输入不能作为物理 Hook 证据：InputFlow 按设计让 injected event 直通。因此又从同一精确 zip 启动
托盘 Agent 和真实 Settings，使用隔离四方向配置完成现场检查。用户确认以下结果均符合预期：

- F8 + Left／Right／Up／Down 分别命中 Copy／Paste／Undo／Redo；
- 距离不足、提前释放和另一鼠标按钮取消不误触发，鼠标正常移动且无粘键；
- F12 暂停时方向规则不触发，再次 F12 后恢复；
- 关闭 Settings 不退出 Agent，最后从托盘正常退出 Agent。

现场进程随后均不存在，`running` marker 已清除。Agent 日志确认 `rules=4`、一次完整
`suspended`／`resumed`，最终为 `observed=299 output_sent=26 output_failed=0 output_dropped=0
hook_panicked=false logger_panicked=false`，没有 `shutdown_limit`。任务卡要求的精确最终包 smoke 至此完成。

### 3.5 RC 结论与后续变更边界

SHA-256 为 `A287489C...004537` 的包已完整通过本轮 RC，RC 阶段判定完成。用户随后要求在实际 release
前增加英语支持补丁，具体要求另行提供；因此当前包只作为已验证 RC 基线，不直接发布。英语补丁涉及
源码、资源或包布局后，必须按任务卡第 4 节第 5 项重新固定提交、生成新包并重复受影响检查，新 manifest、
SHA-256 和 tag 目标取代本记录当前值。

远端发布仍未获具体授权；补丁与新包验收完成后，再按用户发布要求创建 GitHub release。当前没有创建或
推送 tag、分支或 release。

24／72 小时长测属于发布后的 G-POST，不阻塞 RC；代码签名不在本版默认范围。

## 4. 发布边界

RC 完成不等于远端已发布。若没有发布前代码／资源变化，远端 tag 必须指向本记录列出的包构建提交；
附件为 zip、`SHA256SUMS.txt`、版本发布说明和用户指南。当前已经明确计划英语支持补丁，所以必须先让
补丁后的新固定提交和新包完成受影响 RC 检查，再确定实际 tag。没有明确发布要求和授权时不创建或推送
tag／release，也不提前进入发布后的 G-POST。

## 5. RC-01 英语／国际化补丁后的候选重开

2026-10-10，RC-01 补丁的 34 个文件以 commit
`b83f4c368321017f3415f9bb71e0c56b39d3eee6` 固定并推送到 GitHub `origin/main`。旧候选及其物理结果继续
作为历史证据，不改写；新候选重新执行受影响门槛。

从 `HEAD == origin/main == b83f4c3...`、干净工作树执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 `
  -RequireClean -OutputRoot .\target\distribution-rc01-candidate
```

结果：

- 完整门槛通过：Rust 172／172、Protocol 7／7、Settings Core 14／14、WinUI Debug／Release 均
  0 warning／0 error，Debug／Release × en-US／zh-CN 四种运行 smoke 退出 0；
- 候选 ZIP：`target/distribution-rc01-candidate/InputFlow-0.9.0-win-x64.zip`；
- ZIP 94,154,465 bytes，SHA-256
  `DA0DED2CF4162A6A78B12172AAF48AB6FAC799A3C753EDD32F83B44F890A82F7`；
- 解压目录 245,294,323 bytes、总文件数 580，项目 PRI 2,243,768 bytes；manifest commit 匹配、
  `worktree_dirty=false`、MIT、Pre-release、双 self-contained 和私有路径重映射声明正确；
- 精确 ZIP 的全新解压副本通过 Agent 100 轮、Settings en-US／zh-CN、真实 Named Pipe live contract，
  Agent 限时干净退出 0；
- 精确 ZIP 通过 English → 简体中文 → English → System 的真实 UI Automation、偏好持久化和重开链路，
  测试后恢复原偏好，Settings／Agent 进程均不存在。

第一次完整候选命令在 NuGet restore 时被沙箱网络策略拒绝，第一次精确包 Agent 启动也被沙箱拒绝写入
正常 `%LOCALAPPDATA%\InputFlow\agent.log`；在获准网络／当前用户文件权限下以相同提交和输入重跑后通过。
这两项记录为执行环境拒绝，不伪装成产品测试结果。

当前仍未关闭的发布前项目是：未保存草稿和真实规则保存、物理四方向／取消／F12、Narrator／LiveRegion、
125%／150% 缩放、规则编辑页截图，以及可用时的中文／第三语言 Windows 系统匹配。精确步骤和 L01–L16
状态见
[`../releases/patches/RC-01-WinUI3-i18n-en-US-verification.md`](../releases/patches/RC-01-WinUI3-i18n-en-US-verification.md)。
当前结论仍为 `READY FOR RC SMOKE`，不是 `READY FOR RELEASE`；尚未创建 tag 或 GitHub Release。
