# 触控板 Stage 1A 实机记录

2026-11-19（Australia/Brisbane）。ASUS Zenbook 14 UX3405MA，Windows x64 build `26200.9974`。
探针 `0.1.0`，基于仓库 `2530f3b` 工作树，Rust `1.98.1`、`windows-sys 0.61.2`。
本次使用前台注册，未启用 `RIDEV_INPUTSINK`。

## 构建与复现

在项目的 Rust／MSVC x64 开发环境中执行：

```powershell
cargo run --locked --manifest-path .\experiments\touchpad-probe\Cargo.toml
```

前台窗口内按数字 `1` 开始空闲基线、`2` 开始静止轻触和抬起、`3` 开始缓慢滑动，
如有外接鼠标可按 `4` 开始对照。每组按 `Enter` 结束并输出该组计数。`Esc` 关闭窗口。
计数同时写入可执行文件所在目录下的 `touchpad-probe-*.log`，每次启动生成独立文件。

本机离线构建、格式检查、严格 Clippy 及两项 HID 报告批次边界单测通过。

## 枚举与注册

- 当前执行环境的 `Get-CimInstance` 返回拒绝访问（`0x80041003`）。
  改用 `pnputil.exe /enum-devices /connected /class HIDClass /deviceids`，找到状态为 `Started`
  的 `ASUS Precision Touchpad`，硬件 ID 包含 `HID_DEVICE_UP:000D_U:0005`。
- `GetRawInputDeviceList` 返回 17 个设备，其中 11 个为 HID 集合。
  `GetRawInputDeviceInfoW` 识别出 1 个匹配 `0x000D/0x0005` 的集合，VID/PID 为 `093A:300D`，
  设备信息查询错误为 0。
- `RegisterRawInputDevices` 成功。探针窗口完成启动诊断消息分发。
  启动诊断记录 `foreground=false`，实测采样以每组单独记录的窗口焦点为准。
- 命令执行环境首次启动的窗口未出现在操作者桌面。改从桌面会话启动，截图确认窗口可见后再进行采样。

## 前台物理操作

| 操作 | 采样段 | 合计时间 | `WM_INPUT` | 已归因触控板报告 | 各段报告数 |
|---|---:|---:|---:|---:|---|
| 空闲 | 1 | 3.698 秒 | 0 | 0 | 0 |
| 静止轻触并抬起 | 7 | 17.865 秒 | 1,494 | 1,494 | 290、224、229、183、227、215、126 |
| 慢滑并抬起 | 5 | 15.232 秒 | 1,130 | 1,130 | 213、225、179、250、263 |
| 外接鼠标对照 | 未采样 | — | — | — | — |

各段开始和结束时窗口均在前台，段内焦点丢失次数为 0。分组内共取得 2,624 份可归因 HID 报告。
零句柄、来源不明、读取错误和无效批次均为 0。采样段由窗口按键划分。
关闭窗口时的会话总计为 3,759 份，包含未分组的 1,135 份报告。

**1A 结论**　此机器的触控板可通过前台 Raw Input 识别、订阅并读取可归因的 HID 报告。
触控板固件与驱动的该路径结果为本机实测。后台投递和其他设备仍待验证。

## 证据口径

只有 HID 批次通过长度校验且设备句柄可解析成触控板用途时，报告才计入 `touchpad_reports`。
空设备句柄与来源查询失败的报告计入 `unknown_reports`。Win32 允许来自 Precision Touchpad 的
`RAWINPUTHEADER.hDevice` 取零，参见
[微软 RAWINPUTHEADER 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputheader)。
1A 统计报告数量。单指接触、抬起、坐标与持续时间需要后续按 HID 描述符解析验证。
