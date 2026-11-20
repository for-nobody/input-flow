# 触控板 Stage 6 · 虚拟操作面板执行记录

> 文档类型：阶段执行记录
>
> 开工日期：2026-11-20（Australia/Brisbane）

## 开工基线

本地 `main` 和本地 `origin/main` 均指向
`d2dfed92c6e69d63ee54577a841596aacd13e930`。开工时用户已修改 Stage 5 任务卡，Stage 6 任务卡尚未跟踪。
基线为正式配置 Schema v7、Named Pipe v1。使用本机隔离的 Rust 1.98.1 MSVC x64、.NET 10.0.404 和
`scripts/build-windows.ps1`，优先复用缓存依赖。

Stage 5 已在一台 ASUS Zenbook 上用 Num Lock 开、关分别验证板面 2 送达 Windows 计算器与记事本。
全数字扫描码有自动覆盖。当次未采集目标键盘布局。见 [Stage 5 记录](TOUCHPAD_STAGE_5_NUMPAD.md)。

## 实现取舍

- 6A 使用归一化板面坐标。每个区域由凸面片组成。相邻面片可表示凹形和带空白的轮廓，
  只有分属不同区域的公共边形成留白。同一区域的内部接缝隐藏。Rust 与 C# 使用同一套坐标和公差，
  共享斜切和凹口 fixture。每次单指接触只在同一 ID 明确抬手时提交。
- 6B 的 Schema v8 写入完整 `panel` 文档，Named Pipe 继续使用 v1，握手公布
  `touchpad_virtual_panel`。v7 迁移保留经典 3×4、原开关／绑定／提示。显式空列表写入磁盘，
  新 Settings 仍可读写 v4～v7。边界、重叠、重复标识和同作用域绑定由 Agent 校验。
- 6C 保留顶排数字扫描码和原生单指轻点的只读判定。运算用数字键盘运算键，翻页预设用
  PageUp／PageDown。组合键按正序按下、倒序释放，只补发已插入且仍按下的键的释放。
  切层更新输出令牌和提示会话，保持焦点、修饰键、设备及旁路复核。
- 6D 的画布独立持有工作副本与撤销历史。保存到草稿按预设 ID 合并几何和区域内容。
  主窗口沿用现有配置事务。公共边拖动按平移后的线重新裁切相邻区域的并集；
  无法用这条分界线保持两侧有效时，保留原几何并显示原因。

## Windows 桌面证据

Windows x64 build `26200.9974`，同一台可解码 Precision Touchpad 的 HID 有效跨度为
126.9 × 71.2 mm，面板和画布均显示这两个尺寸。隔离命令会话发现 1 台设备，
无权读取该会话用户的单指轻点值，Agent 报告 `native_tap_unknown`。交互用户读回
`TapsEnabled=0`，校验期间未修改系统设置。

- E1／E4（桌面自动化）　在 Release Settings 的规则页进入管理预设，从显式空集合新建布局。
  在独立画布从左上拖至右下，得到两个可选区域。Ctrl+Z 撤销、工具栏恢复均生效。
  通过区域编辑分别绑定 A、B，画布读回两处绑定。
- E5（桌面自动化）　「保存到草稿」自动关窗。主窗口预览出现斜线、显示草稿未保存，
  此时磁盘仍只有空布局，SHA-256 为
  `90820F9F841C3FDC01388DE93A450086AA73908A2D32AA1B53A5F85A10F70ADE`。
  首次主窗口保存后，Agent 记录规则替换，磁盘读回 A／B 和实际小数几何。
  Settings 显示结果待核对，触发保存读回的浮点精度复查。历史试次保留在
  `target/stage6-interactive`。
- 启用 JSON 浮点精确解析后，保留原草稿重连修复后的 Agent。以同一组小数坐标再次正式保存，
  界面显示「保存完成」及验证、落盘、应用和读回通过。新增位级往返回归。
- 在最终 Release Settings 重开同一画布。初始撤销和恢复均不可用，A／B 绑定与原斜边正确。
  直接拖动斜边，使板面归一化点约 (0.67, 0.33) 从 B 侧进入 A。保存到草稿以后，
  主窗口轮廓更新，正式文件的哈希仍为
  `B8761D0CF72991827AA5F307CEE3BD8D0C045B4907E55D74C0BDB96CB62428FC`。
  正式应用和读回后，哈希变为
  `5B7F71B0722DABB6F4CCD4BE05F12E3B30052F13F22C9B57A3AA292C6182E708`。两版几何保存在
  `target/stage6-interactive-final/diagonal-before-edge.json` 和 `diagonal-after-edge.json`。

## 实体触控板主流程

两轮均使用交互用户、当前 HID 板面、原生轻点值 0、记事本目标和 SHA-256 为
`AC7FA633A79BFFCB7725031525948BA45987E4FFC59AC8E070019BEAD0596553` 的 Release Agent。
每轮开始前确认 Settings 已退出。使用英文输入验证字符，记事本目标线程的 HKL 和 Num Lock
未单独采集（本轮不是数字兼容性 O1）。未采集触点的精确坐标。

| 试次 | 实际目标表现 | Agent 计数 |
|---|---|---|
| 原斜边 F9 双击进入，左下及右上轻触 | 操作者确认 `ab`、无异常。记事本 UI Automation 回读 `ab` | 试次前确认／请求／插入为 0／0／0，途中取样 4／8／8，后续进入 Settings 前累计 5／10／10。计数包含这段会话的其他接触 |
| 两级保存移动后的斜边，原 B 位置复触 | 操作者报告最终为 `aba`，原 B 位置可输出 A。中途被动读回出现 `abaa`，因此这轮无法用作单触一次输出的证据 | 试次前确认／请求／插入累计 5／10／10，Esc 退出后为 13／26／26。现场计数没有逐试次归因 |

实际状态记录在 `target/stage6-interactive-final/physical-before.json`、
`physical-a-b.json`、`before-second-tap.json` 和 `physical-moved-edge.json`。
实体试验的 Agent 到期正常退出。最终输出共 14 批，失败和丢弃为 0。这个累计值不用于推断各次触碰的准确输出次数。

## 构建与自动验证

| 验证 | 结果 |
|---|---|
| `scripts/build-windows.ps1 -SkipRestore` | 退出 0。Rust 209 项通过，1 项旧交互覆盖层测试略过，格式和严格 Clippy 通过 |
| C# 协议与 Settings Core | 7 项协议契约和 19 项 Settings Core 契约通过。WinUI Debug／Release 各 0 警告、0 错误 |
| 本地化 | 530 个资源键、134 个 XAML 资源引用，Debug／Release × en-US／zh-CN 四组运行冒烟通过 |
| `cargo test --locked --offline --manifest-path experiments/touchpad-probe/Cargo.toml` | 退出 0，4 项通过 |
| Settings Core `--live-panel`、Protocol `--live` | 退出 0。真实 Named Pipe 上保存含小数顶点的布局、核对应用、恢复原空配置。另验证暂停／恢复、录制取消和断线清理 |
| E2／C1 真实管道与重启 | 使用隔离 v7 文件启动，读回 F7／F8、两秒提示和 12 区经典布局。v7 请求写入收到 `invalid_params`，源文件保留。通过 v8 应用空列表并重启，仍为空 |

`target/stage6-build-verified.log` 为联合构建日志。另保留运行中旧 Agent／Settings 导致的文件占用失败及
重试日志在 `target/stage6-build-final.log`。旧 Agent 到时正常退出，原草稿重连保存后正常关闭 Settings，
再执行完整联合构建通过。
真实管道日志在 `target/stage6-live-wire/core-live.log` 和 `protocol-live.log`。两次试验 Agent 均按时正常退出。
迁移与重启数据在 `target/stage6-migration-live`。

## 验收矩阵进度

| 编号 | 已有证据 | 仍待补测 |
|---|---|---|
| G1 | **通过。** 三次实体触碰分别新增 `a`、`b`、`a`，均为 `+1/+2/+2`。两级保存后原 B 点归 A，记事本最终保存为 `aba` | 历史 `abaa` 的单次归因仍未知，本轮未复现 |
| G2 | Rust 与 C# 共享凹形、接缝、留白 fixture，斜边裁切、边界和无效操作有自动证据 | 实体 L 形内外、共享顶点和全板多区切割的完整现场流程 |
| E1 | 管理页新建、窄窗及最大化、独立画布、A／B 绑定和物理输出 | 中英桌面切换、键盘多选及完整删除与复制流程 |
| E2 | 显式空列表在实际 Agent 应用后重启仍为空，默认目标删除后选其余布局 | 运行中删除、恢复预设和中间绑定释放的完整桌面流程 |
| E3 | 三类拖放、斜边、凹形、碰撞和越界有自动证据，桌面实测独立矩形四边、越边反馈和内部拖至背景 | 同一正式样本的三类拖放，以及 L 形碰撞、斜边取消 |
| E4 | 实际 Ctrl+Z 与工具栏恢复切割，隔离桌面覆盖拖动中公共删除入口、释放捕获和单步撤销／恢复 | 物理 Delete 顺序与完整文本编辑、捕获期间的桌面矩阵 |
| E5 | 草稿保存不落盘、正式应用和读回、重开和默认历史有实测，保存期间画布写回的成功及失败分支有受控测试 | 画布放弃／继续编辑、跨窗口冲突和忙碌窗口两种保存入口的完整现场矩阵 |
| K1 | 三态 `Alt+1/2/3`、双向互斥提示、激光笔样式／临时鼠标／清屏、`F12` 恢复及切层时旧抬手取消有现场证据。真实 Agent 拒绝同作用域重复绑定 | 自定义改绑的 UI 流程 |
| O1 | 顶排数字及有界输出有自动证据，Stage 5 的历史数字 2 实测另保留 | Num Lock 开关两态的指定数字、四式与表格 Enter |
| O2 | Windows Media Player 六键符合操作者预期。PowerPoint 确认三次翻页后 `Esc` 退出面板，放映继续，后台计数对应三次触碰 | 媒体绝对音量与逐动作计数未采 |
| S1 | 双框中途切焦点、重新落指仅交付右框、跨格取消均有目标消息和 Agent 计数。双指符合操作者观察 | 前台跨进程切换、物理按压、窄缝回返与多指的独立试次 |
| S2 | Settings 关闭、实际板面尺寸和始终显示提示有现场证据。解锁后首次双击 `F9` 恢复已修复，离板和保持接触锁屏两轮均通过，旧接触不输出、新接触只交付一次 | 其余提示档、录制／配置应用、睡眠、真实断连和多设备待测 |
| C1 | 真实 Agent v7 迁移、旧模型写保护及重启读回，精确几何往返、落盘失败有自动覆盖 | 旧二进制与新 Settings 的实际配对及物理配置矩阵 |

各次试验和剩余覆盖范围见下方分组记录。设备断连、锁屏及睡眠分别取证。

## 2026-11-21（Australia/Brisbane）· 面板修复与复验

以本地 `main` 的 `a558d7b` 为基线，当时与本地 `origin/main` 一致。开工时已有 Stage 5 任务卡状态改动和未跟踪的 [Stage 6 补充任务卡](../tasks/TOUCHPAD_STAGE_6_PANEL_FIXES_AND_ACCEPTANCE.md)。验证环境为 Windows x64 build `26200.9974`、Rust 1.98.1 MSVC x64、.NET SDK 10.0.404。联合构建在 `target/stage6-followup/verification` 的源码副本执行。桌面试验使用独立编译的 WinUI 画布和测试布局，日常 Agent 和 Settings 未参与写入测试配置。

### 1 · 正式保存期间的画布草稿

- 修复前，Settings Core 回归用异步信号让 `ConfigSaveService.SaveAsync` 冻结包含预设 B 改名的草稿，再暂停于 `ValidateConfigAsync`。此时 A 的边界提交返回成功并进入主草稿。第一次保存返回后，`AcceptSaved` 以只含 B 改名的配置重载主草稿。回归在「提交应被阻止」断言失败，日志为 `target/stage6-followup/before-save-guard.log`，运行退出 `-532462766`。
- `DraftSession.SavePanelCanvas` 现在接受主窗口已有的忙碌状态。在忙碌期间返回 `Panel_SaveBusy`，继续保留按预设 ID 定位、目标已删除和同布局几何冲突检查。`MainPage.Panel.cs` 的公共写回入口服务画布左上角和关窗对话框。`TrySave` 收到错误后显示双语提示，画布和历史仍在。`MainPage.xaml.cs` 在外部变更对话框选择重新加载时，将忙碌状态保持到正式读回完成。
- 修复后，受控成功场景的第一次正式读回仅有 B 的新名称。受控校验失败场景保留旧正式配置和 B 的主草稿改动。两条路径均保留 A 的编辑及撤销，空闲后 A 能提交到主草稿，第二次保存读回同时包含两项改动。两个回归连同既有 19 项 Core 契约共 21 项通过，退出 `0`，见 `target/stage6-followup/after-save-guard.log`。Agent 在本项自动测试中由 `FakeGateway` 代替。
- 待实机：以隔离配置从正式 Settings 主窗口复做一次受控保存，分别操作画布按钮和关窗对话框，核对小图、画布状态、实际 Agent 读回与磁盘两次保存结果。

### 2 · 拖动中删除

在 Windows 桌面以自动化指针拖动已绑定 A 的区域。测试夹具在出现首个有效移动预览后调用实际键盘、工具栏和菜单共用的 `DeleteSelected`，随后由桌面输入继续移动并释放鼠标。修复前，删除之后仍为 `drag=True preview=True`，松手经 `Released` → `UpdateDrag` → `MoveRegion` 抛出 `Sequence contains no matching element`。修复后，`PanelCanvasWindow.cs` 先以 `CancelDrag` 清空拖动和预览并释放捕获，再提交删除。捕获丢失时日志仍有原区域，删除完成及松手后均为空板，拖动与预览均为 `False`。实际按 `Ctrl+Z` 一次恢复原坐标、名称 A 和绑定 A，按 `Ctrl+Y` 一次再次删除。前后日志在 `target/stage6-followup/results/delete-baseline.log`、`delete-fixed.log`。

待实机：从正式画布在按住鼠标时用物理 `Delete` 重做同一序列，补测区域名称输入框和快捷键录制期间的 `Delete`。桌面夹具在拖动期间直接触发公共删除入口，撤销与恢复通过真实键盘路由验证。

### 3 · 独立矩形四边缩放

`PanelGeometryEditor.cs` 识别矩形外边，保留共享边优先命中。`PanelCanvasWindow.cs` 对选择工具的外边拖动走单边缩放，复用校验、预览和撤销历史。`PanelBoard.cs` 根据外边方向显示水平或垂直缩放指针。反馈区保留固定高度，避免拖动时显示或清空反馈导致板面坐标变化。

Settings Core 覆盖四边分别拖动、正交轴移动、标识与动作保留、越界、跨过对边、压零和重叠拒绝、平行拖动无改动、共享边优先，以及无效操作保留恢复分支。指定样例确认右边从 `x=0.45` 到 `x=0.60`，左边仍为 `x=0.20`，点 `(0.52, 0.45)` 从背景变为 A，一次撤销恢复为背景。既有交换、填入未绑定分区、平移和公共斜边用例通过。新增几何用例后共 23 项 Settings Core 契约通过，退出 `0`，见 `target/stage6-followup/after-geometry.log`。

最终 Release 的隔离 WinUI 画布通过桌面拖动验证四边。按右、左、上、下顺序实际读回所拖边为 `x=0.5997`、`x=0.1071`、`y=0.1466`、`y=0.8052`。每步只改变该边，预览和提交的坐标相符，区域仍绑定 A。撤销底边后，尝试把左边拖过右边，界面显示无效反馈，几何和可恢复状态保持。`Ctrl+Y` 恢复底边。随后从区域内部拖入背景，整块平移，原位置释放。撤销平移后点「保存到草稿」，测试回调读到四边缩放后坐标，画布关闭。首次桌面试拖时反馈区高度变化影响落点，稳定反馈区高度后，所见板框在缩放和无效反馈时保持位置。见 `target/stage6-followup/results/resize-final.log`。

待实机：在正式入口复做拖动中按 `Esc`，并核对两级保存后主窗口小图、正式文件与重开的画布。桌面夹具的保存回调只读取提交几何。

### 4 · G1 单触单次交付（待实机）

本轮桌面自动化可操作画布，当前会话无法执行实体触板接触。三次目标文本和计数均待采集。历史中途 `abaa` 属于未按次归因的既有样本，原因仍待确认。

| 试次 | 布局版本与触点定位 | 目标文本前 → 后 | `confirmed` 增量 | `requested_inputs` 增量 | `inserted_inputs` 增量 | 状态 |
|---|---|---|---|---|---|---|
| 原 A 点 | 待采集原版配置哈希和 A 点位置 | 待采集 | 待采集 | 待采集 | 待采集 | 待实机 |
| 原 B 点 | 待采集原版配置哈希和 B 点定位方法 | 待采集 | 待采集 | 待采集 | 待采集 | 待实机 |
| 移边后原 B 点 | 待采集新版配置哈希，按上次定位方式复触 | 待采集 | 待采集 | 待采集 | 待采集 | 待实机 |

最短复验：使用同一 Release Agent、隔离配置和英文记事本，记录设备、进程会话和两版配置的哈希。远离窄缝选 A、B 点，各轻触一次，每次单独读取触碰前后的文本和 `get_status` 的 `result.numpad`。移动斜边、两级保存并关闭 Settings，再在原 B 点轻触一次。采样与切换模式放在触碰区间之外，被动读记事本文本时保留它的焦点。文本每次应分别增加 `a`、`b`、`a`，三项增量每次应为 `+1/+2/+2`。若再出现额外字符，保留原试次读数并沿触点、识别、排队、注入、目标文本逐层复核。

### 5 · O1 数字与四则运算（待实机）

本轮无实体触板试次，未取得键盘布局、Num Lock 前后值、目标读数或 Agent 分次增量。联合构建通过顶排扫描码回归，普通规则的真实小键盘键仍使用既有输出路径。

| 目标 | Num Lock 关闭 | Num Lock 开启 | 每组须核对的实际结果 |
|---|---|---|---|
| 记事本 | 待实机 | 待实机 | 分别得到 `2468`、`505`、`55` |
| Windows 计算器标准模式 | 待实机 | 待实机 | 三串数字正常进入显示框。依次清零并复测 `8+2`、`8−2`、`8×2`、`8÷2` 与面板 Enter，结果为 10、6、16、4 |
| 电子表格 | 待实机 | 待实机 | 活动单元格输入 2 后按面板 Enter，提交一次数值 2 |

最短复验：每组开始前设定 Num Lock，记录布局和锁定状态，再聚焦目标并进入面板。每触分别记录目标文本、显示值或单元格前后值和三项计数增量。每组在切层、输出和退出后核对 Num Lock，保留任何焦点、数值或计数异常。

### 本轮构建结果

| 命令与环境 | 退出码 | 结果 |
|---|---:|---|
| `dotnet restore apps/settings-winui/InputFlow.Settings.slnx -p:Configuration=Debug`，另执行 Release；采用缓存包的 NuGet 配置 | `0` | 联合构建源码副本依赖就绪 |
| `dotnet run --project apps/settings-winui/InputFlow.Settings.Core.Tests/InputFlow.Settings.Core.Tests.csproj --no-restore -c Debug` | `0` | 两项保存时序与两项缩放回归通过，共 23 项 |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build-windows.ps1 -SkipRestore` | `0` | Rust 209 项通过，1 项交互覆盖层测试按默认设置略过。格式、严格 Clippy、探针构建、Release Agent 构建通过。C# 协议 7 项、Settings Core 23 项通过。WinUI Debug／Release 均 0 警告、0 错误。531 个双语资源键、134 个 `x:Uid` 和四组本地化启动冒烟通过 |
| 隔离 WinUI 试验构建 `dotnet build apps/settings-winui/InputFlow.Settings/InputFlow.Settings.csproj -c Release --no-restore` | `0` | 基线和修复后试验程序均为 0 警告、0 错误。前后指针事件结果见第 2、3 项日志 |
| `git diff --check` | `0` | 未报告空白错误 |

初次启动联合构建时，VS 开发命令调用的 `vswhere.exe` 未在进程 `PATH` 中找到，外层 PowerShell 以 `1` 结束。将现有 Visual Studio Installer 目录加入测试进程的 `PATH` 后重试通过，最终完整日志为 `target/stage6-followup/verify-final.log`。

## 2026-11-21（Australia/Brisbane）· 结项现场验收

使用已推送的 `df2d1a2` Release 构建。Agent 的 SHA-256 为 `AEDD84EA4E01F581C29ABC6081CB9C74046805C4D9587362959199ECE3D12ACE`。日常 Agent 经托盘退出后，以隔离配置启动验收实例 PID `12860`，默认布局为数字，包含媒体、翻页和使用左 `Alt+6` 的对角线测试预设。普通规则为空，Touch Click 关闭，激光笔为相对定位。隔离配置初始 SHA-256 为 `1E8C6008B0A25515DCFCC0CAC8BF04625C59BCCB0B211D97453D0FA8EF530AD3`。交互会话读到 `TapsEnabled=0`，同一台笔记本的 HID 有效跨度为 `126.9 × 71.2 mm`。证据位于 `target/stage6-closeout-20261121/`。

### K1 · 模式快捷键与双向互斥

- 普通态左 `Alt+1/2/3` 在双框目标记录到三个数字的系统键按放。面板与激光笔仍为 `ready`。
- 双击 `F9` 后后台依次记录 `numpad`、`media`、`slides`、`numpad`，激光笔状态为 `unavailable/numpad_active`。操作者确认在面板内双击左 Ctrl 时出现先退出面板的提示。
- 激光笔状态记录到 `red-ring`、`yellow-dot`、`blue-ink`、`pointing`、`drawing` 和临时鼠标 `mouse`。蓝笔留下 88 段，清屏后为 0。面板状态为 `unavailable/presentation_active`，操作者确认激光笔内双击 `F9` 可见先退出激光笔的提示。两次 `Esc` 后均恢复普通态。两向互斥期间面板确认和插入计数均为 0，操作者确认无异常。
- 面板活动时 `F12` 暂停退出，第二次 `F12` 恢复为 `ready` 而且面板仍处于未进入状态。目标收到两次 `F12` 按放，左框仍有焦点。键盘布局由 `0x08040804` 切换到英语 `0x04090409`。
- 初次激光笔试验中，目标另收到三个无注入标记、无同区间 Alt 记录的数字，产生 `223`。这些数字发生在首轮样式切换被状态采样捕捉之前，未能对应到已命中的切换序列。随后的红环到黄点两次复查均有状态变化，该时段没有新增目标键消息。保留原样本，不能据此判定发生了已消费热键泄漏。
- 向同一 Agent 发送只读 `validate_config`。基线含自定义 `Alt+6`，`valid=true`。将对角线预设改为与媒体共享的 `Alt+2` 后得到 `valid=false`，错误同时指向 `panel.layouts[media].select` 和 `panel.layouts[diagonal].select`，退出 `0`。结果见 `K1-validate.json`。
- 样本见 `K1-mode-samples.ndjson`、`K1-target.log` 与后续追加的 `mode-samples.ndjson`。接触中切层的旧抬手见下方试次，自定义改绑的正式 UI 流程仍待实机。

### S1 · 双框焦点与取消

目标为双框测试窗口 PID `18780`。数字交付段记录 `HKL=0x08040804`、Num Lock 关闭。初始两个带 Agent 标记的 `6` 进入左框，随后两次 `5` 分别将左框从 `66` 变为 `665`、`6655`，右框一直为空。四个数字各有一组标记按放，累计 `confirmed/requested/inserted` 从 `0/0/0` 变为 `4/8/8`。

| 试次 | 目标文本前 → 后（左／右） | Agent 增量与结果 |
|---|---|---|
| 保持接触，键盘 Tab 从左框切到右框后抬手 | `6655`／空 → `6655`／空 | `0/0/0`。目标报告左框离焦、右框进焦，Agent 增加一次 `focus_changed` 取消 |
| 右框已获得焦点后新轻触 5 | `6655`／空 → `6655`／`5` | `+1/+2/+2`，右框收到带 Agent 标记的按放 |
| 连续接触跨格，以及试次间落在空白或分界 | `6655`／`5` → `6655`／`5` | `0/0/0`。有 `crossed_region` 和 `down_outside_region` 取消证据 |
| 退出前的其他落指 | `6655`／`5` → `6655`／`525` | 两组标记按放和 `+2/+4/+4`。此处未按操作者动作逐一划界，保留样本 |

操作者确认指定的焦点、折返和双指动作符合预期，并指出期间有几次可能因定位而输入其他数字。`S1-target.log` 和 `S1-mode-samples.ndjson` 保留这些数字。双指动作没有单独的采样起止，保留为操作者观察。跨进程前台切换、物理按压和窄缝回返仍待分组复验。

### O2 · 媒体六键和 PowerPoint 放映

在 Windows Media Player 打开三段本地测试音频 `01-first`、`02-middle`、`03-last`。操作者用实体触控板检查上一首、下一首、播放／暂停、音量减、静音和音量加，确认六项结果符合预期。后台采样记录进入 `media`，媒体段前后累计 `confirmed/requested/inserted` 为 `7/14/14` → `15/30/30`，其中包含额外操作，无法按六个动作逐个归因。收到使用 `Esc` 结束的报告，随后状态快照的 `active_layout` 为空。参见 `mode-samples.ndjson`、`O2-current.json`。音量变化为操作者观察，未保留绝对音量读数。

操作者在 PowerPoint 放映中使用 `Alt+3` 切到 `slides`，依次触碰上一页、下一页、下一页，实际翻页符合预期。按 `Esc` 后面板回到 `ready`，放映仍继续。后台观测 `15/30/30` → `16/32/32` → `17/34/34` → `18/36/36`，与三次触碰对应，未见额外确认。

### K1／S2 · 接触中切层与锁屏

- 先在板面保持接触再按 `Alt+2`。Agent 从 `numpad` 切到 `media`，记录一次 `other_input` 取消；抬手、切回数字后均未增加确认或插入（保持 `18/36/36`）。之后一个独立落指向左框正常交付 `5`，操作者在锁屏前清空左框。
- 03:46:00，操作者保持接触并用 `Win+L` 锁屏。Agent 记 `modifiers_held` 取消，变为 `unavailable/session_inactive`，报告设备数从 1 到 0，计数保持 `19/38/38`。03:46:08 解锁后重新订阅一台设备，状态变为 `ready`，没有旧接触输出。双框仍为空。
- **发现恢复异常。** 解锁后多次完整 `F9` 按放到达左框，面板维持 `ready`。两次 `get_stats` 之间 `observed_events` 从 `1331` 增至 `1767`，回调样本从 `13512` 增至 `14462`。03:48:34，双框收到恢复后一次带标记的 `5`，对应 `+1/+2/+2`。操作者确认等待一段时间后恢复，期间也切换了输入法。恢复触发因素待隔离复验。
- 第二轮在手指已离板时从活动面板 `Win+L`，03:51:54 解锁回到 `ready`。03:52:02 和 03:52:12 两轮 `F9` 按放均到达目标。第二轮是在补按并松开 Windows 键、关闭开始菜单之后。两轮都没启动，计数维持 `27/54/54`。日志记录系统在解锁及后续复查时的输入法变化。
- 03:54:16 操作者按放一次字母 `L`，随后双击 `F9` 恢复面板，03:54:19 采样变为 `active`。此之前约两秒还有一次输入法切换。源码的双击起始条件要求内部无仍被视为按住的键或按钮。丢失 `Win+L` 序列中的释放事件与现有结果相符，具体内部残留状态尚无逐键日志证实。
- 证据见 `mode-samples.ndjson`、`double-box.log`、`S2-postunlock-get_status.json`、`S2-postunlock-get_stats.json`、`S2-postunlock-followup-stats.json`。本次只发生锁屏导致的订阅变化。设备物理断连与睡眠各自待实机。

### G1 · 记事本逐触采样（通过）

同一隔离 Agent 和记事本 `G1-notepad.txt`。操作者选择英语输入，切换到 `diagonal`，每次触碰间隔由保存测试文件分开。后台采样持续读文件和 IPC，`Ctrl+S` 在确认后执行。两次目标文本顺序经操作者确认。

| 试次 | 触点 | 目标文本前 → 后 | `confirmed` 前 → 后 | `requested_inputs` 前 → 后 | `inserted_inputs` 前 → 后 |
|---|---|---|---|---|---|
| 03:57:06 原 A | 左下 A 区，板宽约 1/4、板高约 3/4 | 空 → `a` | 34 → 35 | 68 → 70 | 68 → 70 |
| 03:57:19 原 B | 右上 B 区，按板宽约 3/5、板高约 2/5 定位 | `a` → `ab` | 35 → 36 | 70 → 72 | 70 → 72 |
| 04:01:01 移边后的原 B | 以同一板面比例复触原 B 点 | `ab` → `aba` | 36 → 37 | 72 → 74 | 72 → 74 |

两级保存由操作者在隔离 Settings 完成。Agent 记录规则替换，保留的两版文件是 `G1-before-config.json` 和 `G1-after-config.json`。新版 SHA-256 为 `385FF5D3EF2C5DE2E52FBE3423B2C643788D634F64E54A1CDAC625CEC4128429`。归一化样点 `(0.6,0.4)` 在原几何归 B、新几何归 A。关闭 Settings 后单触原 B 点，操作者确认本次只增加一个 `a`，稍后正式保存的记事本文本回读为 `aba`。原始采样见 `G1-target-samples.ndjson`。三次均为 `+1/+2/+2`，本轮未复现历史 `abaa`。

### 结项决定

操作者选择先记录异常并暂缓结项。锁屏恢复项保持未通过。下次修复后，用隔离 Release 构建复做两次 `Win+L`，解锁直接双击 `F9`，验证旧接触无交付及新接触单次交付。其余待实机覆盖见总矩阵。

验收 Agent 经托盘正常退出，累计观察 3,272 个事件、输出 41 批，失败和丢弃均为零，Hook 和日志线程无 panic。采样器与双框测试窗口已退出。日常配置 SHA-256 在验收前后均为 `97D5BDD19798305C794CE4C64DBDB8FBCE65D313A23F97606527D57697285AD9`。恢复日常 Agent 后，PID `29116` 的 IPC 状态为 `ready`、未暂停、1 条启用规则。

## 2026-11-21（Australia/Brisbane）· S2 解锁恢复专项

使用从 `df2d1a2` 导出的源码构建隔离 Release Agent。仅在隔离副本增加诊断和独立数据路径，使用上轮隔离 Schema v8 配置、零条普通规则和同一双框目标。基线与修复版的编译命令均为 `cargo build --release --locked --offline -p inputflow-agent`。原始日志在 `target/stage6-s2/`。

### 修复前 · 第一次 F9 与三个独立对照

开场双击能从 `ready` 进入面板。首轮 `Win+L` 期间，Hook 收到左 Win、L 的 Down，未收到对应的 Up。解锁后首次 F9 Down（序号 17）时，Hook 仍记着左 Win、L 和一个未细记身份的键。Hook 的该条记录还包括刚收到的 F9。匹配器进入 `on_idle_key` 时，F9 无旧按住状态或释放墓碑，`repeat=false`，启动绑定和面板可用，匹配器鼠标按钮集合为空。Win32 对左／右 Win、L、F9 的事件前采样以及五个鼠标按钮采样均为松开。匹配器仍因 `keys.any_physically_held()` 为真跳过双击候选，返回 `PassThrough`，序号 19 同样失败。

| 解锁后的操作 | 日志 | 实际结果 |
|---|---|---|
| 只等待 | 首次与重试 F9 Down（17 → 21）相隔 26.203 秒，无中间规范化事件清除残留。重试时匹配器仍记着原三键。 | 两次都无面板 |
| 只补按 L | 以一次新的锁屏／解锁试次对照。L 的 Down（245）被 Hook 误判为 repeat，Up（246）清除了 L。首次 F9 Down（247）仅剩左 Win，`repeat=false`，理由仍是 `other_key_held`。 | 无面板 |
| 只切换输入法 | 再次从可启动面板开始锁屏／解锁。用任务栏图标切换，双框记录 HKL 从 `0x04090409` 到 `0x08040804`。首次 F9 Down（397）仍残留左 Win、L，理由为 `other_key_held`。 | 无面板 |

三个试次各有独立 WTS 锁屏／解锁记录。基线日志在 `target/stage6-s2/baseline/target/release/s2-data/agent.log`。定向日志只写 Win、L、F9 及其状态，其余内部按住键只计数。F9 Down 的系统采样早于本条事件更新，用于核对上一时刻状态。[Win32 Hook 回调时序](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc)

### 修复

WTS 解锁通知请求 Hook 所有线程同步输入。线程确认输入桌面已回到自身桌面后，按 Win32 的当前按住状态校对用于 repeat 的键集合和匹配器的键／按钮状态。旧候选作废，仍按住的键和鼠标按钮继续阻止新双击，已消费按下仍保留释放归属。触控板继续保持不可用，直到 Hook 完成同步，再恢复 `ready`。原双击规则和时间窗保持原值。

确定性回归覆盖缺失 Win、L 和按钮释放后重新武装，以及实际仍按住的键或按钮分别阻止启动。另一项回归确认旧双击候选被丢弃、仍按住的暂扣键释放继续被消费。两项均已通过。

### 修复后 · 两轮实体触控板验收

修复版隔离 Release Agent 的 SHA-256 为
`DA449D2874CBA81540FD94A2F5192769C0BB659480540D36DF41A6C115C0E96C`。原始 Agent 和双框日志分别位于
`target/stage6-s2/fixed/target/release/s2-data/agent.log`、`target/stage6-s2/double-box-s2-20261120-182709.log`。
双框焦点在左，触碰交付时 HKL 为 `0x04090409`、Num Lock 关闭。

| 试次 | 解锁后第一次 F9 Down | 旧接触、目标文本和确认／请求／插入计数 |
|---|---|---|
| 手指已离板 | Hook 同步清除三项陈旧按住状态，面板随后报告 `ready`。序号 19 为 `eligible`，`repeat=false`，匹配器键／按钮均为空。序号 21 接续候选，第二次 Up 后转为 `active`。 | 锁屏至重开保持 `0/0/0`。新轻触一次，计数 `0/0/0 → 1/2/2`，左框空 → `5`，收到一组带 Agent 标记的按放。 |
| 保持接触时锁屏 | Hook 同步清除两项陈旧按住状态，面板随后报告 `ready`。序号 31 为 `eligible`，`repeat=false`，匹配器键／按钮均为空。序号 33 接续候选，第二次 Up 后转为 `active`。 | 锁屏前一笔独立触碰交付 `2`，本轮基线为 `52` 和 `2/4/4`。锁屏过程中一次候选以 `modifiers_held` 取消。操作者确认旧触点在锁屏后、解锁前松开。到重开时文本与三项计数均未增加。新轻触一次，计数 `2/4/4 → 3/6/6`，左框 `52 → 525`，收到一组带 Agent 标记的按放。 |

两轮均在解锁后第一次正常双击时进入面板，右框保持空白。S2 的锁屏恢复项通过。其余 S2 场景仍按原矩阵复验。

完整 `scripts/build-windows.ps1 -SkipRestore` 与独立探针测试均退出 `0`。Rust 工作区 211 项通过、1 项交互覆盖层测试略过，严格 Clippy 通过。C# 协议 7 项、Settings Core 23 项、四组双语运行冒烟和独立探针 4 项通过。日志见 `target/stage6-s2/verify.log`。

两个隔离 Agent 均经托盘正常退出，无输出失败、丢弃或线程 panic。已恢复原日常 Agent，进程 `39252` 报告就绪及一条规则。日常配置 SHA-256 与验收前一致。
