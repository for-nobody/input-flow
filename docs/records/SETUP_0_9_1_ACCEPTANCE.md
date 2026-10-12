# 0.9.1 Windows 安装程序验收记录

> 文档类型：当前阶段执行记录
>
> 日期：2026-10-12
>
> 范围：`scripts/package-installer.ps1` 生成的 Windows x64 安装程序。不改写 v0.9.0 zip 的 H／RC 证据。

自动构建和安装分项分开记录。

## 1. 构建

本机为 Windows x64 build 26200。打包命令：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-installer.ps1 -Force
```

命令退出码 0。Inno Setup 编译器为 7.1.0。安装目录清单为 53 个文件，另由 `Innoscripts.iss` 拷入 `installer\InputFlow.ico`。发布到自己仓库 Release `v0.9.1` 的附件是：

| 项目 | 值 |
|---|---|
| 文件 | `InputFlow-0.9.1-win-x64-setup.exe` |
| 大小 | 9.25 MB |
| SHA-256 | `96f3449161e371b368cc476827af6e9cfa60275fd72da1d80b6c516e88f5b10b` |

这次构建发生在已经装有 Visual C++ 2015–2022 x64、.NET Runtime 10.0.12 x64 和 Windows App Runtime 2.5.1 x64 的机器上。构建本身不会下载这三份运行库。

## 2. 安装矩阵

2026-10-12，在提交者的沙盒中对发布附件做了下列安装分项，结果均为通过。

| 编号 | 预期 | 结果 |
|---|---|---|
| SET-01 | 安装程序以管理员身份把文件装到 `C:\Program Files\InputFlow`，装好后的 Agent 与设置程序以普通用户身份运行 | 通过 |
| SET-02 | 三份运行库都已存在时，安装程序不下载、不降级 | 通过 |
| SET-03 | 缺少某一份时，安装程序先询问，再下载这一固定版本；拒绝、校验失败或离线缺失时，在复制程序文件前退出 | 通过 |
| SET-04 | 未勾选登录启动时，不创建当前用户 Startup 快捷方式 | 通过 |
| SET-05 | 勾选登录启动时，只创建当前用户 Startup 快捷方式，不创建计划任务、服务或 HKLM Run | 通过 |
| SET-06 | Agent 已在运行时，安装或卸载前需要先从托盘选择 Exit InputFlow | 通过 |
| SET-07 | 开始菜单、托盘 Open Settings 和双击图标都能打开设置程序 | 通过 |
| SET-08 | 卸载保留 `%LOCALAPPDATA%\InputFlow`，也不卸载三份 Microsoft 运行库 | 通过 |

## 3. 图标开关

2026-10-12，提交者在沙盒中按 `docs/guides/BUILD_WINDOWS.md` 第 13 节的两条命令分别打包并安装。图标控制与预期一致。

| 编号 | 命令 | 预期 | 结果 |
|---|---|---|---|
| ICON-01 | `package-installer.ps1 -Force` | Agent 文件图标、窗口、通知区域、安装程序和快捷方式都是系统默认图标；安装目录中没有 `InputFlow.ico` | 通过 |
| ICON-02 | `package-installer.ps1 -Force -UseProductIcon` | 上述位置使用 `installer\InputFlow.ico`，安装目录中有该文件 | 通过 |

## 4. 结论

安装程序能够在上述环境中构建出来。SET-01～SET-08 在沙盒中通过。图标开关 ICON-01、ICON-02 在沙盒中通过。
