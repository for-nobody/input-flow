# RC-01 WinUI 3 i18n/en-US 实施与验证记录

> 日期：2026-10-10（Australia/Brisbane）
> 范围：Settings `en-US`／`zh-CN`、语言偏好、UI Automation、双语文档和发布资源
> 当前结论：**READY FOR RC SMOKE**；不是 READY FOR RELEASE

## 1. Git 基线

- 分支：`main`。
- start commit：`4efe5b9e143bbc8c497d1876502d18483e01465d`。
- end commit：仍为 `4efe5b9e143bbc8c497d1876502d18483e01465d`；本轮没有创建 commit、合并、tag、远端分支或 Release。
- 开始时工作树仅有未跟踪的 `docs/releases/patches/`（用户提供的任务文档）；没有覆盖或丢弃该内容。仓库内未发现 `AGENTS.md`。
- 结束时工作树包含本补丁的 Settings、Core 测试、脚本、README、指南、状态和验证记录改动；这些改动尚未合并，因此还没有可指定为新 RC 的固定提交。
- 补丁前已验证 RC 基线仍是 commit `bc48c36d73b94106d53fe192175dc005f756fe25`、ZIP SHA-256 `A287489C5224DE45685B2B89D799C44472384CF22B12C98E13B3C318BC004537`。它只作为历史对照，不继承为补丁后的 PASS。

## 2. 改动文件与理由

| 范围 | 文件 | 目的 |
|---|---|---|
| 资源 | `InputFlow.Settings/Strings/en-US/Resources.resw`、`Strings/zh-CN/Resources.resw` | 提供英语默认资源和简体中文资源；两套均为 293 个同名键 |
| 资源访问 | `AppResources.cs`、`App.xaml.cs` | 在 XAML 初始化前应用有效语言覆盖；统一动态资源读取和安全的英语缺失资源回退 |
| UI | `MainWindow.xaml(.cs)`、`MainPage.xaml(.cs)`、`Controls/KeyPicker.xaml(.cs)` | 将静态、动态、错误、状态、规则摘要和 UIA 文本迁移至资源；增加三态显示语言设置 |
| 偏好 | `InputFlow.Settings.Core/UiLanguagePreferenceStore.cs` | 将 `system`／`en-US`／`zh-CN` 独立保存到 `%LOCALAPPDATA%\InputFlow\ui-preferences.json`，不改规则 schema |
| 测试探针 | `LocalizationProbe.xaml(.cs)`、`LocalizationSmoke.cs` | 在真实 WinUI/XAML、`ResourceLoader` 和 UIA 属性上验证两套资源，而不只解析 XML |
| 自动测试 | `InputFlow.Settings.Core.Tests/Program.cs`、`scripts/test-localization.ps1` | 覆盖缺失、损坏、不支持值、往返和临时文件清理；校验资源键、`x:Uid`、动态键及硬编码属性 |
| 桌面 smoke | `scripts/smoke-localization-ui.ps1` | 用真实窗口和 Windows UI Automation 验证语言切换、重开、持久化、UIA 名称和回到系统模式；只清理自身启动的 PID，并恢复原偏好字节 |
| 构建／分发 | `InputFlow.Settings.csproj`、`scripts/build-windows.ps1`、`scripts/package-release.ps1` | 设置 `DefaultLanguage=en-US`；让普通构建也携带自包含 Windows App SDK；加入双语运行 smoke、PRI 和双语指南检查 |
| 用户文档 | `README.md`、`README.zh-CN.md`、`docs/guides/USER_GUIDE.en-US.md`、`USER_GUIDE.md` | 提供互链且与当前 v0.9.0 功能、依赖和限制一致的双语入口 |
| 证据 | `docs/releases/patches/assets/RC-01-settings-en-US.png`、`RC-01-settings-zh-CN.png` | 保存本机真实 Settings 页面两种语言渲染结果 |
| 项目记录 | 本任务文档、`docs/status/CURRENT_STATUS.md`、`docs/planning/RELEASE_ROADMAP.md`、`docs/guides/BUILD_WINDOWS.md` | 将 RC-01 状态、复验边界和可重复命令接回现有文档体系 |

Rust remapping、Hook、IPC、规则 schema 和配置文件格式均未改变。Agent 托盘现有文字为英语；本轮没有增加跨进程语言同步，因此不能宣称整个应用随 Settings 的语言选择同步切换。

## 3. 语言解析与配置设计

偏好文件只接受 `system`、`en-US`、`zh-CN`。文件不存在时使用 `system`；JSON 损坏、结构错误、值不受支持或读取失败时安全回退 `system`，只输出不含路径和内容的诊断代码。保存使用同目录唯一临时文件、写穿、`Flush(true)` 和覆盖移动，失败时清理临时文件。

启动顺序为：读取偏好 → 对强制语言设置 `Microsoft.Windows.Globalization.ApplicationLanguages.PrimaryLanguageOverride` → 初始化 XAML。`system` 模式不调用 setter，让 unpackaged 进程使用 Windows/MRT 默认匹配；这是因为当前 SDK 要求 setter 接收有效的单个 BCP-47 标签，实测写入空字符串会导致原生启动失败。unpackaged override 不跨进程持久化，所以每次启动都从应用偏好重新决定。项目 `DefaultLanguage` 为 `en-US`，不支持的系统语言预期回退英语；非中英 Windows 的真实回退仍列为待测。

语言选择保存后只显示当前语言的“重开 Settings 生效”提示，不自动关闭窗口，不停止或重启 Agent。偏好与规则配置分离，未修改 `%LOCALAPPDATA%\InputFlow\config.json`、schema 或协议。

## 4. 屏幕阅读器与无障碍

静态 XAML 使用 `x:Uid`，附加属性资源键使用 `[using:Microsoft.UI.Xaml.Automation]AutomationProperties.Name/HelpText`；动态按钮、规则摘要、状态和验证反馈通过相同 `ResourceLoader` 生成。自动校验覆盖 85 个 `x:Uid`，并检查每个静态及动态键在两套资源中存在。

真实 UI Automation smoke 在英语 Windows 上验证了以下链路：

- 英语系统模式：窗口标题 `InputFlow Settings`、导航名称 `InputFlow Settings navigation`、语言组合框和重开提示为英语；
- 选择简体中文并重开：窗口标题、导航、打开导航按钮、设置项、语言组合框和重开提示均为中文；
- 选择 English 并重开，再选择 System default 并重开：UIA 名称回到英语系统匹配；
- 全程不要求 Agent，偏好文件按测试前状态逐字节恢复。

本机注册了 English (United States) 的 David／Mark／Zira 和 Chinese (Simplified, PRC) 的 Huihui／Kangkang／Yaoyao OneCore 语音，但本轮**没有启动 Narrator、没有验证实际朗读顺序或 LiveRegion 发声，也没有切换或修改系统 TTS 声音**。因此 Narrator 人工项目保持 NOT TESTED。

截图：

- [English Settings](assets/RC-01-settings-en-US.png)
- [简体中文 Settings](assets/RC-01-settings-zh-CN.png)

两张图均来自真实 Debug Settings 窗口。规则编辑页和 Narrator 反馈截图尚未取得，不能由 Settings 页面截图代替。

## 5. 测试结果

环境：Windows NT `10.0.26300.0` x64；系统 locale、当前 culture 和 UI culture 均为 `en-US`；unpackaged WinUI 3、Windows App SDK 2.5.1；测试 commit 为未提交工作树上的 `4efe5b9e...`。

### 5.1 自动、运行时和发布工程验证

| 命令／检查 | 结果 |
|---|---|
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-localization.ps1` | PASS：en-US/zh-CN 各 293 键、键集合一致、85 个 `x:Uid`、动态键和可访问附加属性均通过 |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1 -SkipRestore` | PASS：Rust 172/172；fmt；Clippy `-D warnings`；probe 和 Release Agent；Protocol 7/7；Settings Core 14/14；WinUI Debug/Release 均 0 warning、0 error |
| 上述完整入口内的 WinUI runtime smoke | PASS：Debug/Release × en-US/zh-CN 共 4 次，均从真实可执行文件退出 0 |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-localization-ui.ps1` | PASS：English → 简体中文 → English → System，持久化、重开和 UIA 名称均符合预期，`agent_required=false` |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -OutputRoot .\target\distribution-rc01-i18n-test -SkipBuild -SkipRestore` | PASS（工程包）：发布目录的 en-US/zh-CN runtime smoke 均先退出 0，随后才提升为最终目录和 ZIP；没有残留 staging 目录 |
| 工程 ZIP 回读 | PASS：94,154,457 bytes；SHA-256 `8DE313D1553E0B4E7F40B9FB75BA42EA6BC5032993D9A42A3A4A1F15E2524711`；校验文件一致；PRI 2,243,768 bytes；双语 README 存在 |

该工程包 manifest 如实记录 `git_commit=4efe5b9e...`、`worktree_dirty=true`，只能证明当前发布布局和资源可运行，不能作为最终 RC 或 release 附件。

### 5.2 L01–L16 矩阵

| ID | 状态 | 本轮证据或限制 |
|---|---|---|
| L01 | NOT TESTED | 没有中文 Windows 系统模式环境 |
| L02 | PASS | 英语 Windows `system` 模式真实窗口、标题、导航和 UIA 名称为 en-US |
| L03 | NOT TESTED | 没有非中英 Windows；仅有 `DefaultLanguage=en-US` 和资源探针证据 |
| L04 | NOT TESTED | 没有中文 Windows，不能把英语 Windows 强制 en-US 代替该场景 |
| L05 | PASS | 英语 Windows 选择 zh-CN、关闭重开后，真实窗口和抽样 UIA 名称为中文 |
| L06 | PASS | zh-CN → en-US → system 的三次重开链路通过，最终恢复英语系统匹配 |
| L07 | PASS | Core 自动覆盖偏好缺失、损坏、结构错误和不支持值；UIA smoke 以偏好不存在开始并正常进入 system，规则文件未触碰 |
| L08 | NOT TESTED | 未连接 Agent／加载真实未保存规则草稿；语言选择本身不关闭窗口已由 UIA 验证 |
| L09 | NOT TESTED | Core 保存回归通过，但补丁后真实 Agent 上的规则创建／删除／禁用／保存尚未执行 |
| L10 | NOT TESTED | 补丁后物理键盘／鼠标 remapping 和旁路尚未重跑 |
| L11 | NOT TESTED | UI Automation 抽样名称通过；Narrator 与完整键盘浏览未人工执行 |
| L12 | NOT TESTED | 动态资源读取通过；Narrator 对错误／状态／LiveRegion 的实际朗读未执行 |
| L13 | NOT TESTED | 未在 125%／150% 缩放下完整检查长英文和交互 |
| L14 | PASS | 两套 `.resw` 静态解析、真实 XAML／ResourceLoader／UIA 探针以及 Debug/Release runtime smoke 通过 |
| L15 | NOT TESTED | dirty-worktree 工程发布包通过；尚无固定干净 commit 构建的真实新 RC ZIP，不能冒充最终包 |
| L16 | PASS | 双语 README／用户指南、版本、依赖、命令、限制和互链已审查；未加入虚构下载 URL |

## 6. 与已有 Smoke 的对比

旧 RC 的固定 commit、ZIP 和物理输入结果不因本补丁自动失效为历史事实，但也不能证明新代码通过。当前补丁新增或影响：Settings 启动资源选择、窗口和页面文本、UIA 文本、per-user UI 偏好、普通 Debug/Release 运行布局、发布 PRI 检查和包内文档。

本轮已重新验证资源契约、全部既有自动门槛、四种 Debug/Release 双语运行、真实语言持久化/UIA 链路和 dirty-worktree 发布目录。尚需在补丁固定后重新生成 `-RequireClean` RC 包，并只对该精确 ZIP 完成语言、Narrator、缩放、规则保存和物理 remapping 的受影响实机项目。旧 SHA `A287...` 和本轮工程 SHA `8DE3...` 都不得作为新最终 SHA。

## 7. 剩余问题

### RC smoke 前阻断最终发布，但不阻断进入 smoke

- 补丁尚未提交／合并，没有新固定 RC commit；尚未执行 `package-release.ps1 -RequireClean`。
- L01、L03、L04 所需的中文或第三语言 Windows 环境未提供。
- Narrator 实际朗读、键盘浏览、LiveRegion、125%／150% 缩放、规则编辑页截图未完成。
- 补丁后的真实 Agent 规则编辑／保存、物理键鼠 remapping、F12 旁路和精确新 RC ZIP smoke 未完成。

### 已知非阻断范围

- Agent 托盘仍使用现有英语文字；Settings 语言偏好不会跨进程同步到 Agent。
- 语言切换设计为重开 Settings 生效，不做运行中整页热切换。
- 本轮没有修改系统 Narrator 语音，也没有新增 TTS 引擎。

未发现会阻止构建、启动、资源解析、语言持久化或进入下一轮 RC smoke 的实现缺陷。

## 8. 最终结论

**READY FOR RC SMOKE**

下一步应先人工审查并固定本补丁提交，再从该干净提交构建新的 RC ZIP；所有剩余项目必须针对新 commit 和新 SHA 执行。当前状态不是 READY FOR RELEASE，且本轮没有执行任何远端发布动作。
