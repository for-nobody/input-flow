# InputFlow 修复任务 · 规则首页滚动与托盘打开设置

请在 Stage 5 完成后的 checkout 上处理以下两项 Windows 用户反馈。按 A、B 分别复现、修改和验收。优先使用当前仓库的构建脚本、正式 UI 和托盘入口。测试配置与日常配置隔离，保留用户已有的规则与未保存草稿。

## A · 小窗口里无法读完规则首页

**用户场景**　在设置页启用实验性触控板支持并保存，回到规则页后缩短窗口。开启激光笔和数字键盘时，多张卡片会占满工作区，即使有普通规则，也可能看不到第一条。

**已确认的布局原因**　`MainPage.xaml` 的 `RulesListPane` 有四行。Touch Click、激光笔、数字键盘各占一行 `Auto`，普通 `RulesList` 与空列表提示在最后的 `*` 行。三张卡片在列表滚动范围之外。Stage 4 有两行卡片，Stage 5 新增了第三行。`PresentationEditor` 和 `RuleEditorScroll` 的滚动只服务各自的编辑界面。[规则页 XAML](../../apps/settings-winui/InputFlow.Settings/MainPage.xaml) · [列表刷新和宽窄布局](../../apps/settings-winui/InputFlow.Settings/MainPage.xaml.cs)

**修复目标**　让用户能从第一张可见卡片一直滚到最后一条普通规则；没有普通规则时也能滚到空状态。优先考虑让现有 `RulesList` 持有有限高度，把实验卡片放进 `ListView.Header`，将空状态放进同一滚动范围。保留普通规则列表的虚拟化与编辑、删除、开关事件。若用其他布局，请说明滚轮落在卡片上时由谁滚动，以及长列表如何保持有限视口。

标题、筛选和「验证并保存」继续可用。保留激光笔编辑器的独立滚动，以及普通规则和快捷键编辑器现有的宽屏并排、窄屏覆盖行为。轮询刷新触控板状态时，用户正在看的位置和键盘焦点应稳定。[激光笔卡片和编辑器](../../apps/settings-winui/InputFlow.Settings/MainPage.Presentation.cs) · [数字键盘卡片](../../apps/settings-winui/InputFlow.Settings/MainPage.Numpad.cs)

| Windows 桌面验收 | 通过条件 |
| --- | --- |
| 保存触控板总开关，分别仅开总开关和同时启用两个子功能。将窗口缩短到卡片内容超过视口 | 从卡片区域用滚轮或触控板双指连续上下滚动，能读完状态、访问数字键盘控件并回到顶部 |
| 在窄窗口及 150% 显示缩放下复验。分别准备空列表与多屏普通规则 | 空状态和最后一条规则均可到达。键盘 Tab 可将屏外操作带入视野，停留数秒不会被状态刷新拉回顶部 |
| 打开普通规则、激光笔预设和数字键盘快捷键编辑，分别在宽窄窗口完成或取消一次，再保存有效草稿 | 相应编辑器可操作，返回后可访问规则列表；筛选、取消和「验证并保存」语义正确，重开能读回保存内容 |

## B · 托盘点击 Open Settings 后没有窗口

**已确认的启动路径**　菜单与双击均发送 `TrayEvent::OpenSettings`。Agent 调用 `launch_settings`，使用显式 `--settings PATH` 或默认的 Agent 同目录 `InputFlow.Settings.exe`。文件缺失或创建子进程失败时，托盘分支只写日志；`settings launch requested` 仅证明 `spawn()` 成功。Stage 4 与 Stage 5 的托盘分发、路径解析、启动函数及 Settings 单实例激活代码一致。[托盘事件](../../crates/inputflow-windows/src/platform/shell.rs) · [Agent 启动器](../../apps/inputflow-agent/src/main.rs)

**先区分运行方式**　开发构建把 Agent 放在 `target/release`，将 Settings 构建到 WinUI 项目的 `bin/<配置>/<目标框架>/win-x64`。直接启动前者、同目录没有 Settings、也没有传 `--settings` 时，可以确定会走缺文件分支。便携包脚本将两者放在同一个发布目录。分别验证这两种入口，并记录托盘实际所属的 Agent PID 与路径。[开发构建](../../scripts/build-windows.ps1) · [便携包构建](../../scripts/package-release.ps1)

在用户点击一次菜单前后比对时间、进程与日志。以下 PowerShell 只读，可在普通用户会话运行。

```powershell
Get-CimInstance Win32_Process -Filter "Name='inputflow-agent.exe'" |
    Select-Object ProcessId, ExecutablePath, CommandLine
Get-Process -Name 'InputFlow.Settings' -ErrorAction SilentlyContinue |
    Select-Object Id, Path, MainWindowHandle, MainWindowTitle
Get-Content -LiteralPath (Join-Path $env:LOCALAPPDATA 'InputFlow\agent.log') -Tail 40
```

开发构建还可做一次单变量对照。在仓库根目录、没有现存 Agent 的测试会话中，先用同一份 Release Agent 默认启动并点击托盘。通过托盘 Exit 结束该次测试，再运行以下命令并点同一菜单。第二次成功且第一次留下缺文件日志，即可把该环境中的故障归到路径解析。已运行的主 Agent 不会因启动第二个 Agent 而更换它自己的 Settings 路径。

```powershell
$uiExe = (Resolve-Path -LiteralPath '.\apps\settings-winui\InputFlow.Settings\bin\Release\net10.0-windows10.0.26100.0\win-x64\InputFlow.Settings.exe').Path
& .\target\release\inputflow-agent.exe --settings $uiExe
```

| 点击后观察到的证据 | 下一步与修复目标 |
| --- | --- |
| `open_settings: settings executable is unavailable` 或 `failed to launch settings` | 核对日志中的目标或显式参数，直接启动该份完整 Settings 作对照。开发模式给出能直接运行的联合启动入口或命令，把本次构建的 Settings **绝对路径**传给首个 Agent；同步修正开发指南。便携模式继续按 Agent 自身位置解析。缺文件或启动失败时给用户一条看得见、能采取行动的提示，并保留日志。 |
| 有 `settings launch requested`，之前没有 Settings，点击后子进程很快退出 | 记录子进程退出情况和 Windows 应用程序错误。用同一份 Settings、相同工作目录直接启动作对照，核对随附的 PRI、XBF、DLL 和图标。另从不同工作目录启动 Agent 对照。`MainWindow` 使用相对图标路径，是否影响本次启动要靠对照结果判定。[窗口初始化](../../apps/settings-winui/InputFlow.Settings/MainWindow.xaml.cs) |
| 有启动记录，之前已有 Settings，但最小化或后台窗口没有回到用户面前 | 现实现按会话互斥，再按当前语言的窗口标题 `FindWindow`，最多尝试 20 次；第二实例随后以 0 退出。测试最小化，以及保存新显示语言、旧窗口仍开着时再次点击。若命中此分支，用与显示标题无关的方式定位本会话 Settings、恢复原窗口并使激活失败可诊断；保留原 PID 和草稿。[单实例处理](../../apps/settings-winui/InputFlow.Settings/SettingsInstance.cs) · [启动分支](../../apps/settings-winui/InputFlow.Settings/App.xaml.cs) |
| 同一时间段没有启动或失败记录 | 对照右键菜单、双击及同一菜单里的 Pause/Resume；确认所点图标对应的进程。必要时给菜单命令选择与 Agent 收到事件增加临时诊断，定位事件消失的位置。 |

**Windows 桌面验收**　使用同一提交分别跑以下场景。每项记录实际入口、Agent/Settings PID、窗口表现及对应日志。

1. 完整便携目录位于含空格或中文的路径；从其他工作目录启动 Agent。Settings 关闭时点击托盘菜单应出现可用窗口；关闭 Settings 后 Agent 继续运行，双击托盘图标能重新打开。
2. Settings 已经打开或最小化时，再次点击恢复原窗口，已有未保存编辑仍在。托盘 Pause/Resume 仍可工作。
3. 干净的开发构建按更新后的开发入口启动，点击托盘能打开本次构建的 Settings。在测试副本中指定不存在的 Settings，确认有可见反馈、日志可定位，Agent 继续工作。

历史版本有真实桌面托盘打开 Settings 的验收记录。本次交付请附上修复前后各自的复现条件、最小改动和实测结果。若当前便携包正常、只复现了开发目录问题，请明确写出这个边界。无法访问 Windows 交互桌面时，请将对应的点击、滚动和窗口激活验收标记为待实机验证。[历史托盘验收](../archive/m7/M7_PHASE_E_AND_M6_ACCEPTANCE.md)
