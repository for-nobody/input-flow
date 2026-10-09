# 首版发布 RC 执行记录

> 文档类型：当前阶段事实记录
>
> 状态：执行中
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

第一次从固定提交生成的候选包通过构建、IPC 和桌面启动检查，但进一步解压审计发现三个自研二进制
仍含构建用户绝对路径：两个 .NET 项目引用 DLL 保留 CodeView/PDB 记录，Rust Agent 保留标准库与
Cargo 源位置。该包及其 SHA-256 已作废，不能发布。修复方案是为 Release WinUI 解决方案统一关闭
调试目录、为 Release Agent 重映射工作区／用户路径，并把二进制路径扫描固化为打包失败条件；脏
工作树工程包已证明扫描通过。

RC 仍在执行。修复提交固定后必须从新的干净提交重新运行完整入口；最终记录将列出固定提交、工具链、
完整命令、测试数、包大小、文件数、SHA-256、manifest 回读、自动 smoke、真实桌面 smoke 与环境限制。

## 4. 发布边界

RC 完成不等于远端已发布。远端 tag 必须指向本记录最终列出的包构建提交；附件为 zip、
`SHA256SUMS.txt`、版本发布说明和用户指南。没有明确授权时不创建或推送 tag／release，也不提前进入
发布后的 G-POST。
