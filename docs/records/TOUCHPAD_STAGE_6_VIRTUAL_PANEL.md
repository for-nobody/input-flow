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
| E1 | 管理页新建、窄窗及最大化、独立画布、A／B 绑定和物理输出 | 中英桌面切换、键盘多选及完整删除与复制流程 |
| E2 | 显式空列表在实际 Agent 应用后重启仍为空，默认目标删除后选其余布局 | 运行中删除、恢复预设和中间绑定释放的完整桌面流程 |
| E3 | 三类拖放、斜边、凹形、碰撞和越界有几何与契约测试 | 同一样本的三种真实桌面拖放 |
| E4 | 实际 Ctrl+Z 与工具栏恢复切割，自动覆盖取消保留恢复分支和新操作清理 | 完整文本编辑及捕获期间的桌面矩阵 |
| E5 | 草稿保存不落盘、正式应用和读回、重开和默认历史有实测 | 画布放弃／继续编辑及跨窗口冲突的完整现场矩阵 |
| K1 | 普通／激光笔／面板三个快捷键作用域有 matcher 测试。F9 与 Esc 用实体键完成主流程 | 各层 Alt 组合、双向互斥提示、F12 及旧 UP 的实机组合 |
| O1／O2 | 顶排数字及有界输出测试，Stage 5 的历史数字 2 实测另保留 | 开关 Num Lock 分别输入指定数字串和四式，表格回车，媒体六键和放映翻页 |
| S1／S2 | 记事本实测、实际感应面尺寸和始终显示提示，焦点／取消模型和部分插入测试 | 双框中途切焦点、四档提示、其他设备、断连、锁屏与睡眠 |
| C1 | 真实 Agent v7 迁移、旧模型写保护及重启读回，精确几何往返、落盘失败有自动覆盖 | 旧二进制与新 Settings 的实际配对及物理配置矩阵 |

待实机复验使用任务卡给出的各编号操作，优先在同一笔记本隔离配置中按顺序完成 K1、O1、S1，
再用有播放列表和非首尾页面的目标执行 O2。S2 的多设备、断连、锁屏和睡眠分别取证。

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
