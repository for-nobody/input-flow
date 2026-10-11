# RC-01 WinUI 3 i18n/en-US 实施与验证记录

> 日期：2026-10-10～2026-10-11（Australia/Brisbane）
> 范围：Settings `en-US`／`zh-CN`、语言偏好、UI Automation、Narrator、缩放、双语文档和发布资源
> 当前结论：**READY FOR RELEASE**；等待明确的远端发布授权

## 1. Git 基线

- 分支：`main`。
- start commit：`4efe5b9e143bbc8c497d1876502d18483e01465d`。
- i18n 实现 commit：`b83f4c368321017f3415f9bb71e0c56b39d3eee6`（`feat: localize WinUI settings for RC`）。
- Narrator 校验错误修补 commit：`71dca0e4cb3b704b486c1906087537fd3e083b59`。
- 最终候选 commit：`c18a0a80f004451f98bb80bfb7b90c66255845bb`（补充 Pause／Resume 明确通知与焦点恢复）。
- 开始时工作树仅有未跟踪的 `docs/releases/patches/`（用户提供的任务文档）；没有覆盖或丢弃该内容。仓库内未发现 `AGENTS.md`。
- i18n 实现和两次无障碍修补均已直接提交到 `main` 并推送至 GitHub `origin/main`；没有创建 merge commit、tag 或 Release。
- 最终候选构建前已确认 `HEAD == origin/main == c18a0a8...` 且工作树干净；候选 manifest 记录 `worktree_dirty=false`。
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
- 全程不要求 Agent，偏好文件按测试前状态逐字节恢复；同一链路随后又针对精确 RC ZIP 的解压副本通过。

本机注册了 English (United States) 的 David／Mark／Zira 和 Chinese (Simplified, PRC) 的 Huihui／Kangkang／Yaoyao OneCore 语音。本轮使用 Narrator 完成中英文键盘浏览、控件名称、错误 LiveRegion、语言重开提示和 Pause／Resume 状态通知实测；两项首轮缺陷修补后均由最终精确候选回归通过。测试没有切换或修改系统 TTS 声音。

截图：

- [English Settings](assets/RC-01-settings-en-US.png)
- [简体中文 Settings](assets/RC-01-settings-zh-CN.png)
- [最终候选 125% 英文规则错误态](assets/RC-01-c18a0a8-125-en-rule-editor-error.png)
- [最终候选 125% 英文设置页](assets/RC-01-c18a0a8-125-en-settings.png)
- [最终候选 150% 英文设置页](assets/RC-01-c18a0a8-150-en-settings.png)
- [最终候选 150% 中文规则错误态](assets/RC-01-c18a0a8-150-zh-rule-editor-error.png)

前两张图来自真实 Debug Settings 窗口；后四张来自 commit `c18a0a8` 的精确 ZIP 解压副本，窗口分别由
120 DPI（125%）和 144 DPI（150%）确认。Narrator 结论来自实际朗读，不用视觉截图替代听觉结果。

## 5. 测试结果

环境：Windows NT `10.0.26300.0` x64；系统 locale、当前 culture 和 UI culture 均为 `en-US`；unpackaged WinUI 3、Windows App SDK 2.5.1；最终固定候选 commit 为 `c18a0a8...`。

### 5.1 自动、运行时和发布工程验证

| 命令／检查 | 结果 |
|---|---|
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-localization.ps1` | PASS：en-US/zh-CN 各 293 键、键集合一致、85 个 `x:Uid`、动态键和可访问附加属性均通过 |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1 -SkipRestore` | PASS：Rust 172/172；fmt；Clippy `-D warnings`；probe 和 Release Agent；Protocol 7/7；Settings Core 14/14；WinUI Debug/Release 均 0 warning、0 error |
| 上述完整入口内的 WinUI runtime smoke | PASS：Debug/Release × en-US/zh-CN 共 4 次，均从真实可执行文件退出 0 |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-localization-ui.ps1` | PASS：English → 简体中文 → English → System，持久化、重开和 UIA 名称均符合预期，`agent_required=false` |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -OutputRoot .\target\distribution-rc01-i18n-test -SkipBuild -SkipRestore` | PASS（工程包）：发布目录的 en-US/zh-CN runtime smoke 均先退出 0，随后才提升为最终目录和 ZIP；没有残留 staging 目录 |
| 工程 ZIP 回读 | PASS：94,154,457 bytes；SHA-256 `8DE313D1553E0B4E7F40B9FB75BA42EA6BC5032993D9A42A3A4A1F15E2524711`；校验文件一致；PRI 2,243,768 bytes；双语 README 存在 |
| `package-release.ps1 -RequireClean -OutputRoot .\target\distribution-rc01-candidate` | PASS：从 `b83f4c3` 干净提交重新运行完整门槛；Rust 172/172、Protocol 7/7、Settings Core 14/14、Debug/Release 0 warning／0 error、四种双语 runtime smoke 均通过 |
| 干净候选回读 | PASS：目录 245,294,323 bytes；ZIP 94,154,465 bytes；总文件数 580；PRI 2,243,768 bytes；manifest commit 匹配且 `worktree_dirty=false` |
| 精确 ZIP 解压副本自动 smoke | PASS：Agent 100 轮退出 0；Settings en-US／zh-CN 各退出 0；真实 Named Pipe live contract 通过并由 Agent 限时干净退出 0 |
| 精确 ZIP 解压副本 UIA smoke | PASS：English → 简体中文 → English → System；持久化、重开、窗口／导航／控件 UIA 名称均通过，原偏好恢复 |
| `package-release.ps1 -RequireClean -OutputRoot .\target\distribution-rc01-c18a0a8-candidate` | PASS：从 `c18a0a8` 干净提交完成 Rust 172/172、Protocol 7/7、Settings Core 14/14、WinUI Debug/Release 0 warning／0 error及四种双语 runtime smoke |
| 最终候选 ZIP 回读 | PASS：94,155,158 bytes；SHA-256 `D2C1BE6148E2F3833DEEA9C99D9C882F0BABD2A030A1CFD480D1364387D79D2C`；580 个条目、245,295,355 bytes、PRI 2,243,776 bytes、50 个第三方许可文件、禁止条目 0 |
| 最终精确 ZIP 自动回归 | PASS：Settings en-US／zh-CN、English → 简体中文 → English → System UIA、Agent 100 轮及真实 Named Pipe live contract 均退出 0；偏好、进程和 running marker 清理完成 |

早期工程包 manifest 如实记录 `git_commit=4efe5b9e...`、`worktree_dirty=true`，只保留为开发证据。`b83f4c3`
和 `71dca0e` 的两个干净候选因实测 Narrator 缺陷撤销。唯一最终候选为
`target/distribution-rc01-c18a0a8-candidate/InputFlow-0.9.0-win-x64.zip`，SHA-256：
`D2C1BE6148E2F3833DEEA9C99D9C882F0BABD2A030A1CFD480D1364387D79D2C`。测试封装或沙箱权限造成的失败均以
修正后的同输入重跑确认，没有记录为产品 PASS 或 FAIL。

### 5.2 L01–L16 矩阵

| ID | 状态 | 本轮证据或限制 |
|---|---|---|
| L01 | NOT TESTED | 没有中文 Windows 系统模式环境 |
| L02 | PASS | 精确候选 ZIP 在英语 Windows `system` 模式的真实窗口、标题、导航和 UIA 名称为 en-US |
| L03 | NOT TESTED | 没有非中英 Windows；仅有 `DefaultLanguage=en-US` 和资源探针证据 |
| L04 | NOT TESTED | 没有中文 Windows，不能把英语 Windows 强制 en-US 代替该场景 |
| L05 | PASS | 精确候选 ZIP 在英语 Windows 选择 zh-CN、关闭重开后，真实窗口和抽样 UIA 名称为中文 |
| L06 | PASS | 精确候选 ZIP 的 zh-CN → en-US → system 三次重开链路通过，最终恢复英语系统匹配 |
| L07 | PASS | Core 自动覆盖偏好缺失、损坏、结构错误和不支持值；UIA smoke 以偏好不存在开始并正常进入 system，规则文件未触碰 |
| L08 | PASS | 精确候选连接真实 Agent；切换中文后窗口不自动关闭且草稿仍在，本地化未保存提示的“返回”与“不保存退出”均符合预期，退出后规则文件未改变 |
| L09 | PASS | 隔离配置上创建、禁用、保存、关闭重开、删除和恢复均符合预期；最终恢复 4 条启用规则，Agent 权威配置一致且语言偏好未进入规则 schema |
| L10 | PASS | 四方向物理 remapping、距离不足、提前释放、右键取消、F12 暂停／恢复、普通输入和关闭 Settings 后 Agent 继续运行均符合预期；25 个输出批次无失败或丢弃 |
| L11 | PASS | 中英文完整键盘浏览、导航、规则控件名称、输入框和重开提示均符合预期；后续修补只触及动态操作通知和操作按钮焦点恢复 |
| L12 | PASS | 最终 `c18a0a8` 精确候选中，中文空规则 ID 错误及英语 Pause／Resume 状态均被 Narrator 正确朗读；焦点不再跳入规则列表 |
| L13 | PASS | 125%／150% 下中英文设置页、规则编辑器、错误态和长文本无裁切、重叠或横向滚动；所有控件与底部按钮经滚动可达，证据截图已保存 |
| L14 | PASS | 两套 `.resw` 静态解析、真实 XAML／ResourceLoader／UIA 探针以及 Debug/Release runtime smoke 通过 |
| L15 | PASS | `c18a0a8` 最终 ZIP 的 checksum、manifest、PRI、双语指南、许可、禁止条目、双语运行、UIA、Agent 100 轮与 live IPC 全部回读通过 |
| L16 | PASS | 双语 README／用户指南、版本、依赖、命令、限制和互链已审查；未加入虚构下载 URL |

## 6. 与已有 Smoke 的对比

旧 RC 的固定 commit、ZIP 和物理输入结果不因本补丁自动失效为历史事实，但也不能证明新代码通过。当前补丁新增或影响：Settings 启动资源选择、窗口和页面文本、UIA 文本、per-user UI 偏好、普通 Debug/Release 运行布局、发布 PRI 检查和包内文档。

本轮已重新验证资源契约、全部既有自动门槛、四种 Debug/Release 双语运行，并从固定干净提交生成过
候选。SHA `DA0DED2C...A82F7` 的语言持久化／UIA、Agent 100 轮、双语资源、live IPC、规则保存和
物理 remapping 实机项目已通过，但中文 Narrator 校验错误播报暴露出发布阻断缺陷，因此该 ZIP 已撤销
最终候选资格。第一项修补提交 `71dca0e` 生成的 ZIP SHA
`AB9B5E0D...5658AB8` 通过了中文错误播报回归，但英语 Pause/Resume 状态没有被朗读，因此也已撤销。
第二项修补固定为 `c18a0a8` 后已从干净提交重建候选；精确包状态播报、125%／150% 缩放、双语运行、
UIA、Agent 100 轮和 live IPC 均通过。旧 SHA `A287...`、工程 SHA `8DE3...`、`DA0DED2C...A82F7` 和
`AB9B5E0D...5658AB8` 均不得发布。

### 6.1 Narrator 校验错误缺陷与修补

首次中文实测中，规则 ID 为空并选择“写入草稿”后，视觉错误正确出现，但讲述人没有可靠朗读。根因是
规则编辑器 `EditorInfo` 没有声明 LiveRegion，动态更新后也没有显式通知 UI Automation。修补为：

- 将错误摘要声明为 `Assertive` LiveRegion；
- 把本地化标题和正文合成为动态 UIA 名称；
- 显示错误后显式发送 `LiveRegionChanged`；
- 在本地化契约脚本中固定上述两项无障碍要求。

修补版 Release 中重复空 ID 操作后，讲述人已正确读出中文错误；没有保存草稿或修改正式规则。

### 6.2 Pause/Resume 状态通知缺陷与修补

`71dca0e` 精确候选的英语测试中，Pause 和 Resume 操作本身成功，但讲述人转而朗读规则列表，没有朗读
“Agent paused”或“Agent resumed”。原因是 `OperationBar` 在标题和正文更新前先被打开，普通
`LiveRegionChanged` 仍可能被异步操作导致的焦点变化覆盖。修补为：

- 先更新标题、正文、严重级别和动态 UIA 名称，再显示消息；
- 操作结束后重新启用并恢复原暂停／恢复按钮焦点；
- 使用 `RaiseNotificationEvent` 的 `ActionCompleted`／`ActionAborted` 与 `ImportantMostRecent`，向屏幕
  阅读器发送明确的操作结果；
- 语言重开提示也显式发送 LiveRegion 事件，并在本地化契约中固定操作通知要求。

修补版英语 Release 中，讲述人已依次正确朗读暂停与恢复结果，焦点不再跳入规则列表，Agent 最终恢复
`ready` 且 `suspended=false`。

## 7. 剩余问题

### 环境不可得项目（非阻断）

- L01／L04 保留 `NOT TESTED`：没有中文 Windows 系统显示语言环境。
- L03 保留 `NOT TESTED`：没有非中英 Windows 系统显示语言环境。

### 已知非阻断范围

- Agent 托盘仍使用现有英语文字；Settings 语言偏好不会跨进程同步到 Agent。
- 语言切换设计为重开 Settings 生效，不做运行中整页热切换。
- 本轮没有修改系统 Narrator 语音，也没有新增 TTS 引擎。

未发现会阻止构建、启动、资源解析、语言持久化或发布的实现缺陷。

## 8. 最终结论

**READY FOR RELEASE — AWAITING REMOTE PUBLISH AUTHORIZATION**

最终 `c18a0a8` 候选已完成全部可用环境发布前检查。L01、L03、L04 因系统语言环境不可得保持
`NOT TESTED`，不以强制资源覆盖冒充系统匹配，也不阻断本次公开测试版。当前没有创建 tag 或 Release。

## 9. 最终候选与发布边界

已撤销候选（仅保留历史证据，不得发布）：

- commit：`b83f4c368321017f3415f9bb71e0c56b39d3eee6`
- ZIP：`target/distribution-rc01-candidate/InputFlow-0.9.0-win-x64.zip`
- SHA-256：`DA0DED2CF4162A6A78B12172AAF48AB6FAC799A3C753EDD32F83B44F890A82F7`
- 已解压自动证据目录：`target/acceptance/rc01-b83f4c3-package/InputFlow-0.9.0-win-x64`

第二个已撤销候选：

- commit：`71dca0e4cb3b704b486c1906087537fd3e083b59`
- ZIP：`target/distribution-rc01-71dca0e-candidate/InputFlow-0.9.0-win-x64.zip`
- SHA-256：`AB9B5E0DA1E1A8084B8A0B3181E816051B8350514A4BDAD8E9503777C5658AB8`
- 原因：英语 Pause/Resume 操作结果没有被讲述人朗读

唯一可发布候选：

- commit：`c18a0a80f004451f98bb80bfb7b90c66255845bb`
- ZIP：`target/distribution-rc01-c18a0a8-candidate/InputFlow-0.9.0-win-x64.zip`
- SHA-256：`D2C1BE6148E2F3833DEEA9C99D9C882F0BABD2A030A1CFD480D1364387D79D2C`
- checksum：`target/distribution-rc01-c18a0a8-candidate/SHA256SUMS.txt`
- 精确解压副本：`target/acceptance/rc01-c18a0a8-package/InputFlow-0.9.0-win-x64`
- ZIP／解压大小／条目数／PRI：94,155,158／245,295,355 bytes／580／2,243,776 bytes

人工测试严格使用候选 ZIP 的全新解压副本，结果如下：

1. **L08 草稿边界（PASS）**：连接候选 Agent 后形成未保存草稿，切换中文没有关闭窗口且草稿仍在；
   本地化未保存提示的返回与不保存退出均符合预期，退出后规则文件未改变。
2. **L09 配置回归（PASS）**：隔离配置副本上的创建、禁用、保存、关闭重开、删除和恢复均符合预期；
   最终配置恢复为 4 条启用规则，Agent 读回一致，语言偏好没有写入规则 schema。
3. **L10 输入回归（PASS）**：四方向动作、距离不足、提前释放、右键取消、F12 暂停／恢复、普通输入和
   关闭 Settings 后 Agent 继续运行均符合预期。消息目标捕获 C／V／Z／Y 与 1 次上下文菜单；Agent
   `observed_events=362`、`output_batches_sent=25`、`failed=0`、`dropped=0`，最终 `ready` 且未暂停。
4. **L11／L12（PASS）**：英语和简体中文的完整键盘浏览、UIA 名称、错误提示、重开提示以及最终候选
   Pause／Resume 通知均符合预期；没有修改系统语音设置。
5. **L13（PASS）**：125% 和 150% 下英文长文案、中文错误态、规则编辑器和设置页无裁切、重叠或不可达
   控件；没有横向滚动，四张最终候选截图已固化。
6. **环境项**：有中文 Windows 时执行 L01／L04；有第三语言 Windows 时执行 L03。环境不可得则保持
   `NOT TESTED` 并写明系统 build／显示语言，不用强制资源覆盖冒充系统匹配。

最终自动回归后 Settings／Agent 进程均为 0，语言偏好恢复为测试前不存在，running marker 已清除；Agent
100 轮与 live contract 的停止记录均为 `output_failed=0`、`output_dropped=0`、无 hook/logger panic。
发布时 tag `v0.9.0` 必须精确指向 `c18a0a8`，附件只使用上列 ZIP 和 `SHA256SUMS.txt`，并标记 Pre-release。
