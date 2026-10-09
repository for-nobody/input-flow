# InputFlow 用户指南（Windows x64 公开测试版）

InputFlow 是一个本地运行的键盘与鼠标组合工具。`inputflow-agent.exe` 常驻并拥有托盘、输入 Hook、
配置和 IPC；`InputFlow.Settings.exe` 只在需要配置时运行。首版是公开测试版本，尚未完成 24／72
小时长时间运行验收。

## 首次运行

1. 将整个 zip 解压到一个普通用户可写目录。不要只复制两个 EXE；包内 DLL 和资源必须保持完整。
2. 运行 `inputflow-agent.exe`。它会在通知区域显示 InputFlow 图标。
3. 双击托盘图标或从托盘菜单选择 **Open Settings**。
4. 首次启动没有启用的演示规则，不会主动拦截正常输入。

本包面向 Windows x64，声明的最低系统版本为 Windows 10 build 17763；首版实际分发验收范围以发布
说明列出的系统为准。目录包同时携带 .NET 与 Windows App SDK 运行组件，无需单独安装这两项；
但必须安装 Microsoft Visual C++ Redistributable 2015–2022 x64：
<https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist>。本版未签名；系统如显示来源或
信誉提醒，请核对发布页 SHA-256，不要关闭系统保护。

## 创建方向规则

在 Settings 新建 Mouse Direction 规则，选择激活键、方向、动作，然后设置：

- 最小距离：屏幕像素；例如 80 px。
- 最长时间：毫秒；例如 500 ms。
- 偏轴容差：屏幕像素；例如 40 px。

保存后，按住激活键并移动鼠标。光标移动始终直通；每个按住周期最多触发一次动作，松开后才能重新
武装。规则失败、提前释放或超时会尽力回放被暂扣的激活键。

## 暂停、关闭和退出

- `F12`：紧急暂停／恢复。
- 托盘菜单：显示权威状态，并可暂停、恢复、打开 Settings 或退出 Agent。
- 关闭 Settings：只关闭设置程序，Agent 与规则继续运行。
- **Exit InputFlow**：正常退出 Agent；若有已消费按键仍处于按住状态，最多等待两秒接收释放。
- 强制结束进程不能保证恢复此前已经暂扣的历史输入。

输出受目标完整性（UIPI）、焦点和当前修饰键状态影响；非提升 Agent 不能保证向提升窗口注入动作。

## 可选登录自启动

自启动默认关闭，只为当前用户启动 Agent，不启动 Settings，也不创建服务或计划任务。

- 启用：运行 `Enable-InputFlow-Autostart.cmd`。
- 查看：运行 `Get-InputFlow-Autostart-Status.cmd`。
- 禁用：运行 `Disable-InputFlow-Autostart.cmd`。

脚本管理当前用户 Startup 目录中唯一的 `InputFlow Agent.lnk`，重复启用不会创建第二个入口。如果移动
或升级程序目录，请从新目录重新运行启用脚本，以更新快捷方式目标。

## 升级

1. 关闭 Settings，并从托盘正常退出旧 Agent。
2. 将新 zip 完整解压到一个新目录；不要在程序运行时逐个覆盖 DLL／EXE。
3. 默认保留 `%LOCALAPPDATA%\InputFlow`，其中包含配置、有限备份和本地日志。
4. 如果使用自启动，从新目录重新运行启用脚本。
5. 启动新 Agent，打开 Settings，核对规则、禁用状态和方向参数；确认后再删除旧程序目录。

Schema v4 包含鼠标方向。早于 v4 的旧 Agent／Settings 不支持这些规则；不要让旧版本打开后保存 v4
配置。当前版本如果发现高于 v4 的配置，会以无规则旁路模式启动、显示不支持版本的错误，并拒绝保存，
以免旧版覆盖未来格式；请退出并改用支持该 Schema 的版本。降级前先备份专属数据目录，并使用明确支持
当前 Schema 的版本。

## 移除

1. 关闭 Settings，从托盘正常退出 Agent。
2. 运行 `Disable-InputFlow-Autostart.cmd`。
3. 确认没有 InputFlow 进程后，删除解压出的完整程序目录。

默认保留 `%LOCALAPPDATA%\InputFlow`，便于以后恢复规则。只有明确不再需要配置和日志时，才手动删除
这个专属目录。不要删除共享 Visual C++ Redistributable 或其他软件的文件。

## 配置、诊断与反馈

- 配置与日志：`%LOCALAPPDATA%\InputFlow`。
- 默认日志不记录逐键 identity；只有显式诊断参数 `--debug-input` 才记录详细输入。
- 反馈问题时请提供 InputFlow 版本、Windows build、规则类型与参数、复现动作、目标程序是否提升，
  以及去除敏感信息后的相关日志。不要上传私人配置或完整输入轨迹。

已知未完整验证项包括多屏／跨 DPI 热插拔、部分特殊键硬件、中文 Narrator 实际语音、partial
`SendInput`、睡眠／唤醒和 24／72 小时长测。
