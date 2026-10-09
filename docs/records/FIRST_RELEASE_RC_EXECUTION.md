# 首版发布 RC 执行记录

> 文档类型：当前阶段事实记录
>
> 状态：执行中（固定包与自动检查完成；待最终物理输入 smoke）
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

自动输入不能作为物理 Hook 证据：InputFlow 按设计让 injected event 直通。因此任务卡第 4 项中的最终
四方向真实命中、失败／取消和 F12 暂停恢复仍需用户在这个精确 zip 上完成；既有 Phase F／H 物理证据
继续有效，但不冒充本次最终包 smoke。完成该短检查前，RC 不标记完成。

### 3.5 当前剩余项

1. 对上述 SHA-256 对应的最终 zip 完成一次物理输入 smoke，并记录观察与 Agent 最终健康日志。
2. 用户若授权远端发布，再让 `v0.9.0` tag 指向构建提交 `bc48c36d...`，创建 GitHub Pre-release 并上传
   zip、`SHA256SUMS.txt`、发布说明与用户指南。

24／72 小时长测属于发布后的 G-POST，不阻塞 RC；代码签名不在本版默认范围。

## 4. 发布边界

RC 完成不等于远端已发布。远端 tag 必须指向本记录最终列出的包构建提交；附件为 zip、
`SHA256SUMS.txt`、版本发布说明和用户指南。没有明确授权时不创建或推送 tag／release，也不提前进入
发布后的 G-POST。
