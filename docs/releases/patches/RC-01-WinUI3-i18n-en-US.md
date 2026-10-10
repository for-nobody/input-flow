# InputFlow RC-01 — WinUI 3 国际化与英语补丁实施任务

> **文件位置**：`docs/releases/patches/RC-01-WinUI3-i18n-en-US.md`
> **任务类型**：首个公开 Pre-release 前的有限范围 RC 补丁（i18n + a11y + README）
> **日期**：2026-10-10（Australia/Brisbane）
> **状态**：本地实现和自动／UIA 工程验证已完成；等待新的 RC 实机 smoke，详见[验证记录](RC-01-WinUI3-i18n-en-US-verification.md)。
> **任务名称**：`RC-01 WinUI3 i18n/en-US`
> **语言约定**：本文和执行记录使用中文；代码标识符、注释、测试名称和 commit message 使用英文；最终用户可见文本用 `.resw` 本地化。

## 0. 交付定义与优先约束（必读）

在 InputFlow 进入首个 Release Candidate 的 Smoke Test 阶段，为已存在的 **C# + WinUI 3 Settings** 添加完整的英文 `en-US` 与简体中文 `zh-CN` 本地化，并在设置里提供语言选择及系统语言自动匹配。同步维护英文 GitHub 首页 README 和中文 README；对 Windows Narrator / UI Automation 暴露的用户可读文本进行同语言本地化。

**本次补丁必须交付**：

1. 语言资源：`Strings/en-US/Resources.resw`、`Strings/zh-CN/Resources.resw`，两者键集合一致。
2. Settings 界面：`System default / 跟随系统`、`English`、`简体中文` 三个选择项，选择可持久化，重启 Settings 后生效；允许显示切换生效提示，提示本身同语言。
3. 自动匹配：系统模式使用 **Windows 的应用/显示语言资源匹配机制**，按当前实际打包方式确认行为；中文匹配 `zh-CN`，英语匹配 `en-US`，未支持的语言回退英语。
4. 所有 Settings 用户可见文字：导航、窗口标题、按钮、说明、表单、占位符、规则编辑、错误/确认、提示、空状态、状态栏、诊断及通知文字，不残留仅中文的硬编码资源。
5. 无障碍：屏幕阅读器读到的 `AutomationProperties.Name`、`HelpText`、输入框标签、图标按钮名称、动态状态通知与语言标记，应与 **InputFlow 当前选定的 UI 语言**一致。
6. 仓库根部的 `README.md` 改为真实、可用的英文首页；`README.zh-CN.md` 保留且更新中文文档；两者互链。
7. Debug/Release 构建与最终分发目录都具备必要语言资源；完成自动测试 + Windows 实机 Narrator 和双语言 Smoke Test。
8. 形成逐项实施与验收记录，更新相关 RC 文档入口；**不得修改历史 RC / Smoke 测试结果或冒充已经测试**。

**明确不做**：新增 remapping 规则类型、触控板新能力、URL / 启动程序动作、IPC 协议变更、核心引擎重构、规则 Schema 升级、整体 UI 重设计、运行数十小时的 G-POST 长测、新增 TTS 语音引擎、修改 Windows 系统 Narrator 的默认语音设置。

## 1. 入手前必须重新核对仓库现状

当前助手没有拿到这轮 RC 的完整最新工作树，因此以下目录是已知结构参考，不是对当前提交的假定。CodeX 必须先执行：

```powershell
git status --short
git branch --show-current
git rev-parse HEAD
```

阅读实际存在的 `AGENTS.md`（根和目标目录）、`00_InputFlow-文档交接与执行入口.md`、`docs/RELEASE_ROADMAP.md`、`docs/BUILD_WINDOWS.md`、`README.md`、`Steps.md`，并核对最近的 RC Smoke 记录、Settings C# 源码与部署脚本。

重点检查：

- `apps/settings-winui/InputFlow.Settings/` 当前页面、`App.xaml(.cs)`、`MainWindow`/`MainPage`、设置存储方式、是否已有 `.resw`、项目 `.csproj` 和打包方案。
- `apps/settings-winui/InputFlow.Settings.Core/` 有无可复用的用户设置抽象；不要将 **显示语言偏好**塞进 Rust 规则 Schema。
- `apps/inputflow-agent/` 与托盘菜单是否有用户可见字符串。**仅做范围审计**：若可安全小规模本地化，可以纳入；若需要改造跨进程语言同步，则另行列出实际剩余工作，不得宣称全应用全部本地化完成。
- 构建、publish、Installer/ZIP 的具体形式：MSIX packaged、unpackaged、self-contained、framework-dependent 或其它。必须以实际脚本与最终产物为准，不能用早期规划替代现实。
- `README.md` 的现有功能、平台与依赖声明是否已更新到当前 RC。不得依据旧版规划文件臆造 Release 支持范围。

如存在未提交修改，禁止覆盖、`reset --hard`、强制 cherry-pick 或删除用户工作；若当前 Smoke 正在进行，先保存其 commit / 产物基线，再在独立工作分支实现（建议 `feat/rc01-i18n-en-us`）。

## 2. 文件布局和作用域

目标目录示意（**以仓库已有约定为准**）：

```text
inputflow/
├── README.md                               # English: GitHub default entry
├── README.zh-CN.md                         # 简体中文，保留现有功能说明
├── apps/
│   └── settings-winui/
│       └── InputFlow.Settings/
│           ├── Strings/
│           │   ├── en-US/Resources.resw
│           │   └── zh-CN/Resources.resw
│           ├── App.xaml / App.xaml.cs      # 语言初始化（按实际文件）
│           └── ...                        # 现有页面 + 新的语言 UI
└── docs/
    └── release/
        └── patches/
            ├── RC-01-WinUI3-i18n-en-US.md             # 本文
            └── RC-01-WinUI3-i18n-en-US-verification.md # CodeX 实施后新建
```

不要因为添加补丁，复制一套 Settings 页面，或引入 WPF / Qt / WebView / Electron / 第三方国际化框架。优先 WinUI 3 / Windows App SDK 原生 MRT Core `.resw` 路线。

## 3. 语言策略（必须保持可预测）

### 3.1 UI 选择项和配置

保存一个独立的用户界面设置字段，推荐语义如下：

```json
{
  "language": "system"
}
```

允许值只有 `system`、`en-US`、`zh-CN`。上述 JSON 只是**语义示例**；实际读取/写入方式应复用仓库已有的 Settings 偏好持久化位置，不得在代理端的规则配置 Schema 中直接新增该字段。

- `system`（默认）：使用 Windows 的正常 UI 语言资源选择；不保留人工强制覆盖。
- `en-US`：强制英文界面（包括 Narrator 可访问名称）。
- `zh-CN`：强制简体中文界面（包括 Narrator 可访问名称）。
- 用户未配置语言或旧版本升级：默认 `system`，不删除原有规则。
- 用户偏好不可读、损坏、不支持的值：安全回退 `system` 或受支持的 `en-US`，记录**无敏感数据**的诊断，不阻止 Settings 启动。
- 选择项名称固定使用本国语言以保证可找回：`System default / 跟随系统`、`English`、`简体中文`，但每一项的说明和可访问名称须与当前 UI 语言一致。
- 保存必须可靠，避免异常退出后留下半写入文件；尽可能复用现有原子持久化策略。

### 3.2 启动和优先级

**优先级**：有效的人工选择 > `system` 的 Windows 语言匹配 > 无匹配时 `en-US` 回退。

在任何 XAML UI 字符串和 `ResourceLoader` 资源初始化前，读取偏好并设置相应的 `Microsoft.Windows.Globalization.ApplicationLanguages.PrimaryLanguageOverride`：

```csharp
// Illustrative pseudocode: adapt to the current App startup lifecycle.
string preference = LoadLanguagePreference(); // system | en-US | zh-CN
ApplicationLanguages.PrimaryLanguageOverride = preference switch
{
    "en-US" => "en-US",
    "zh-CN" => "zh-CN",
    _ => ""
};
// Only now initialize XAML and create the first view/resource loader.
```

**注意**：代码示例中的 `ApplicationLanguages` 来自 `Microsoft.Windows.Globalization`，不要把它与名字相似但持久化行为不同的 `Windows.Globalization` API 混用；准确的 using/初始化位置必须由 CodeX 在现有项目编译验证。对于 **unpackaged** Settings，override 进程外不会自动持久化，所以必须自行保存语言偏好，并在**每次启动**前重新应用。**packaged** 可能自动持久化 override，但仍以统一偏好存储为应用内选择的单一真相来源。

不要简单用 `CultureInfo.CurrentUICulture` 或 `Thread.CurrentThread.CurrentCulture` 作为唯一资源定位方案，因为 WinUI 3 的 `.resw` 属于 MRT 资源体系；需要保证 XAML 和 C# 动态字符串由同一套实际生效的语言上下文解析。系统模式尽量交给 MRT 默认上下文，不在应用进程内随意全局覆盖键盘区域设置或系统 locale。

**打包差异**：微软文档特别注明 unpackaged 应用的默认资源语言解析采用**系统显示语言**，而 packaged 场景会利用应用语言偏好机制。不要假定 `en-AU` / `zh-TW` / 非中文 locale 的回退等行为未经测试就必然一致。对非 `zh-CN` 系统语言，以本次支持列表与英文 fallback 为最终测试目标；在 RC 测试报告中写清实际环境。

### 3.3 切换行为

- 推荐 RC 方案：选择 -> 保存偏好 -> 显示本地化的“重新打开 Settings 生效”提示 -> 用户关闭并重新打开 Settings；**Rust agent 继续运行**。
- 如果实现“重新启动 Settings”按钮，只能重启 Settings 自身，不杀死 agent、不卸载 Hook、不清空未保存的规则编辑草稿。
- 更改语言时如存在未保存的规则，沿用原先的保存/取消/离开确认机制；绝不静默丢失编辑内容。
- 不要求本补丁实现无重启的完全热切换。选择完成后不能出现混合语言假象；重开后必须全局统一。
- 在 `system` 模式下更换 Windows 系统显示语言后，需要重新打开 Settings 才要求生效。

## 4. `.resw` 迁移与字符串规则

### 4.1 创建、引用与检查

使用官方约定：

```text
InputFlow.Settings/Strings/en-US/Resources.resw
InputFlow.Settings/Strings/zh-CN/Resources.resw
```

**示例 XAML**：

```xml
<TextBlock x:Uid="GeneralPageTitle" />
<Button x:Uid="AddRuleButton" />
```

**对应资源键（两份 `.resw` 均需有）**：

```text
GeneralPageTitle.Text    = General / 常规
AddRuleButton.Content    = Add rule / 添加规则
```

上述表格左右内容分别填入两个独立文件，**不是把英文/中文拼在一条字符串里**。CodeX 应按工程真实控件类型填写属性后缀，如 `TextBlock.Text`、`Button.Content`、`TextBox.PlaceholderText`，避免 `x:Uid` 与属性不匹配导致运行时报错。

动态字符串统一通过 Windows App SDK 的 `Microsoft.Windows.ApplicationModel.Resources.ResourceLoader`（或当前现有的、经过验证的相同资源系统封装）获取。不要凭经验复用 `.resx` 的本地化机制。

- 完整盘点所有页面及组件的硬编码用户文本；包括 C# 中构造的 `ContentDialog`、通知、校验结果、诊断状态、空列表与错误消息、捕捉按键提示等。
- 内部事件名、命令行参数、协议字段、枚举序列化值、配置键和稳定日志 ID 不翻译。
- UI 展示给用户的 key 名称可本地化描述，但**物理键身份、扫描码、键名序列化**不得改变。
- 英语需自然、简洁、统一：`Trigger`, `Action`, `Rule`, `Enable`, `Disable`, `Hold`, `Key chord`, `Mouse movement`, `Pause`, `Resume`, `Capture`, `Emergency bypass` 等统一术语。
- 不在资源中添加固定宽度以适配英文；优先检查响应式布局、自动换行、视图滚动、DPI 125%/150%、较大文本。
- 带参数的本地化错误信息不得字符串拼接造成词序错误，应使用两个语言各自的格式化模板，并对变量使用安全格式化。
- 资源键命名稳定且有语义，如 `RuleEditor_SaveButton.Content`、`RuleValidation_ConflictMessage`；禁止相同键跨语言承担不同语义。

### 4.2 缺失资源安全性

- 新建脚本/单测：读取两份 `.resw` 的 XML 并比较资源键集合，检查重复 key、空值、无法加载、非法 `x:Uid` 引用。
- 英文 `en-US` 是默认回退语言；所有应翻译的字符串都必须在两套资源中有显式条目。
- 对于偶发资源丢失，优先使用可理解的默认英文提示，不在正式用户 UI 中展示仅供开发使用的裸资源键。
- 添加最小化资源装载 Smoke：在真实 Debug 和 Release 构建中读取至少一个 `.resw` 的 UI 属性字符串、C# 动态消息字符串和 UIA 自动化字符串，确认不是只检查 XML 是否存在。

## 5. Windows Narrator / 辅助功能（与语言完全联动）

本补丁的目标是**让 Windows 屏幕阅读器读取与当前 InputFlow UI 一致的文本并提供正确语言信息**。我们不制造新的朗读系统，也不强行更改 Windows Narrator 的语音配置。

### 5.1 必须本地化的辅助文本

审计所有交互组件的可访问名称和提示：

- `AutomationProperties.Name`：无文字图标按钮、切换按钮、加号/删除、复制、导入/导出、规则上下移动、窗口操作；优先复用可见且已经本地化的标签，避免重复冗余。
- `AutomationProperties.HelpText`：快捷键捕获区、规则条件、保存冲突说明、错误恢复、语言选择器辅助描述。
- `AutomationProperties.LabeledBy`：文本框/组合框若有可见 label，优先指向同一标签，确保可见与朗读内容一致。
- 动态通知：保存成功、校验失败、断开/重连 agent、启用/暂停、无效规则、捕获等待等；确保 Narrator 能注意到必要状态变化，不能仅靠颜色提示。
- tooltip、控件描述、访问键说明和窗口标题均不能残留另一种语言。

在 `x:Uid` 的 `.resw` 资源中，本地化 `AutomationProperties.Name` 必须使用 WinUI 3 对应的附加属性限定键：

```text
AddRuleIconButton.[using:Microsoft.UI.Xaml.Automation]AutomationProperties.Name
```

而非 UWP 的 `Windows.UI.Xaml.Automation` 旧命名空间。以当前 Windows App SDK 版本和运行验证为准。

**示例**：两份资源各为一个无文本图标按钮提供 `Add rule` / `添加规则` 朗读名；在 C# 动态更新可访问名时也从当前资源读取，而不是硬编码英文。

### 5.2 UI 语言标记和朗读语音边界

- 使页面/根 UI 与子元素具有对应 `en-US` / `zh-CN` 的语言信息，优先依赖 WinUI 3 默认资源语言传播；对需明确设置的自定义控件/文本适当使用 `FrameworkElement.Language`，**以 UIA / Narrator 实测结果决定**。
- 手动选择英语后，打开 Settings，Narrator 应读取英文的按钮名称、菜单、错误和状态；选择中文后读取中文名称。
- Windows Narrator **能够在存在相应 TTS voice 的情况下**自动选用文本语言对应的语音；缺少语音时可能使用默认音色读出文字。补丁不承诺能替用户下载英语/中文语音包，也不更改 Narrator 系统音色配置。
- 若发现特定 WinUI 控件/Windows 构建无法仅依据 `FrameworkElement.Language` 推动 Narrator 切换音色，必须写明复现环境和限制；不得以修改 Windows 系统语音偏好代替正确的本地化。
- 本次验收至少检验**读出的文本与 UI 语言一致**、UIA 可访问名称不缺失；有适当中英语音的环境再额外测试自动语音切换，并客观记录。

### 5.3 Live Region 与焦点

- 既有动态状态有必要时使用 `AutomationProperties.LiveSetting="Polite"`（严重错误可按实际语义评估 `Assertive`）；确保状态变更后触发可被屏幕阅读器感知的通知事件。
- 不以频繁 `Assertive` 中断用户；禁止因语言补丁改变键盘 Tab 顺序、焦点管理或已有输入捕获退出通路。
- 对于动态的 Narrator 提示，用两种资源文件解析真实 UI 语言后再发布通知；不要缓存另一语言的旧字符串。

## 6. README 双语言交付（必须同步）

**本补丁应直接在代码仓库更新两份真实文件**，而不是仅附加一份翻译提案：

- `README.md`：英文 GitHub 默认首页，以**当前最新真实代码与 RC 分发状态**为依据完整重写/翻译现有中文内容。
- `README.zh-CN.md`：完整保留现有中文说明，与英文版同步校准，不丢失实用构建命令、限制和贡献说明。

建议两个文件顶部互链：

英文：

```md
[English](README.md) | [简体中文](README.zh-CN.md)
```

中文：

```md
[English](README.md) | [简体中文](README.zh-CN.md)
```

两份 README 至少都包含：

1. InputFlow 简介：Windows 键盘/鼠标映射和组合输入工具；Settings 与后台 agent 分离。
2. **已实现**的功能与使用实例（只描述当前仓库实测实现）；不将未交付的触控板扩展、URL、启动程序等未来设想写成已实现。
3. 当前支持的平台/架构、用户需要的运行依赖、安装与启动方式；使用实际 RC 构建记录。
4. 规则编辑、启用/暂停、紧急旁路与输入恢复使用方式及安全注意事项（以当前实现为准）。
5. 设置语言：默认自动匹配 Windows；用户可从 Settings 选择英语或简体中文；更改后重启 Settings 生效；Narrator 读取本地化标签，实际语音需依赖 Windows TTS。
6. 从源代码构建与测试方法（Rust + WinUI 3），路径/命令与实际工程一致。
7. Release / Pre-release 下载与注意事项：**只能指向真实存在的发行物或相对路径**；尚未发布时不捏造 GitHub Release URL。
8. 限制 / Known issues / 贡献和 Issue 反馈方式（按已验证事实）。
9. 使用 MIT License 的说明：只有当仓库确实已有 LICENSE 文件时链接之；若 LICENSE 尚待落地，提交单独提醒，不擅自覆盖原许可证。

README 不翻译代码示例中的程序键名、文件路径和 CLI 参数；翻译语义说明即可。两种语言的 feature list 与安装命令保持事实一致。不要修改 `docs/` 中历史中文规划和已验收记录的原始含义。

## 7. 分发与构建资源（RC 高优先级）

确认工程实际采用 packaged 还是 unpackaged 部署（以前者的应用清单和后者的 `.pri` 资源机制分别执行）：

1. 资源应被正确编译进 `.pri`/应用包或生成到运行时可访问路径；`dotnet build` 通过不意味着 installer/ZIP 资源齐全。
2. **unpackaged**：先检查现有 SDK 构建目标是否已自动生成/携带正确 `.pri`。若没有，参照官方 MRT Core 指南接入适当的 `MakePri.exe` 生成/复制流程；不要机械地无条件重复运行 MakePri 或改坏 SDK 原有目标。
3. **packaged**：在实际存在的 appx/MSIX manifest 中明确声明/生成支持语言，并测试默认语言回退。
4. 若发布流程涉及单文件 .exe / self-contained / zip / 安装器，逐项验证 `.pri` 与依赖实际落盘；未验证不能宣称支持独立分发。
5. 使用发布目录**直接启动** `InputFlow.Settings.exe`（或真实入口），不能只在 VS 调试器里测试本地化。
6. 新设置偏好保存在已存在的 per-user Settings 配置位置；不要写入只读安装目录，不要将偏好绑定到 portable 介质上的全局规则 schema。
7. 不因语言功能增加 agent Hook 处理路径的文件 I/O、进程间等待或锁竞争。

## 8. 实施流程（按顺序，实施后写结果）

### I18N-0：基线采集

- [x] 保存当前 git HEAD、分支、dirty files、现有 RC smoke 基线、发布包类型。
- [x] 盘点页面和用户可见文本，列出需迁移文件、关键资源键与 A11y 缺口。
- [x] 形成与现有项目约束不冲突的具体实施计划；仓库内未发现 `AGENTS.md`。

### I18N-1：资源系统

- [x] 两套 `.resw`；设置英语默认回退；完成全部静态字符串迁移。
- [x] C# 动态字符串迁移；资源键对齐、语法校验、资源装载测试。
- [x] 双语言 Settings 页面实际渲染通过；已保存两种语言截图。

### I18N-2：设置与持久化

- [x] `system` / `en-US` / `zh-CN` 三选项出现在真实 Settings。
- [x] 设置自动保存并跨 Settings 重开保持；旧配置兼容。
- [x] 读取偏好与设置 override 发生在加载任何应用字符串资源之前。
- [x] 取消强制覆盖后真正回到跟随系统，而非继续沿用旧语言。
- [ ] 有未保存规则时切换/重启 Settings 不丢数据；agent 不中断。

### I18N-3：Narrator / UIA

- [x] 所有图标按钮、表单字段和动态通知具有符合语言的可访问文本。
- [x] `.resw` 正确使用 `[using:Microsoft.UI.Xaml.Automation]`。
- [ ] 两种 UI 语言下的 Narrator + 键盘浏览、错误通知/LiveRegion 实测完成。
- [ ] 若语音引擎未安装，真实报告限制，不调用系统 API 自动更改用户 Narrator 语音。

### I18N-4：README 与分发

- [x] 英文 `README.md` + 中文 `README.zh-CN.md`，互链且互不矛盾。
- [x] 更新真实安装/依赖/构建命令、版本范围和 Release 声明。
- [x] 更新 build/publish 流程并验证工程发布输出包含资源。
- [x] 在发布路线和当前状态中加入本补丁追踪入口。

### I18N-5：回归和 RC 基线更新

- [x] 运行既有 Rust、.NET、协议测试与 Settings smoke；记录数量、命令、结果和环境。
- [ ] 两种语言各进行手工 UI + Narrator + 映射规则回归。
- [ ] 在干净机器/隔离环境测试真正发布包（如无法完成，明确标记未验证，不能算 PASS）。
- [x] 写验证记录并列出未覆盖限制。
- [x] 补丁提交后指定新的 RC 候选 commit／产物 hash；剩余人工 smoke 仍须针对该精确候选完成。

## 9. RC Smoke Test 矩阵（不得全部以单元测试替代）

| ID | 场景 | 预期 | 类型 |
|---|---|---|---|
| L01 | 中文 Windows + System | `zh-CN` 页面/辅助名称 | Windows 实机 |
| L02 | 英文 Windows + System | `en-US` 页面/辅助名称 | Windows 实机 |
| L03 | 非中英 Windows + System | 默认回退英文；无缺失资源 | Windows 实机或记录限制 |
| L04 | 中文 Windows 强制 en-US，关闭后重开 | 全英文 UI、UIA/Narrator 标签 | Windows 实机 |
| L05 | 英文 Windows 强制 zh-CN，关闭后重开 | 全中文 UI、UIA/Narrator 标签 | Windows 实机 |
| L06 | 从 en-US/zh-CN 切回 System | 下次启动恢复系统匹配 | Windows 实机 |
| L07 | 旧版本偏好不存在/非法 | 可启动，安全 fallback，不修改规则 | 自动 + 实机 |
| L08 | 本地规则编辑未保存时切换 | 规则草稿保留或明确确认，不静默丢失 | Windows 实机 |
| L09 | 规则创建/删除/禁用/保存 + agent 运行 | 与补丁前一致 | 自动 + 实机 |
| L10 | 键盘与鼠标输入 remapping/旁路 | 不因 UI 语言改变事件行为 | Windows 实机 |
| L11 | Narrator 浏览设置、列表、图标按钮 | 名称可理解、顺序与标签正确 | Windows 实机 |
| L12 | Narrator 读错误/状态变化 | 当前语言、无过度重复或中断 | Windows 实机 |
| L13 | 125%/150% 缩放及较长英文文案 | 无裁切、重叠或不可访问交互 | Windows 实机 |
| L14 | Debug/Release `.resw` 资源 | en-US、zh-CN 键存在且读取正确 | 自动 |
| L15 | 真实 RC 的 publish / installer / ZIP | 从分发目录加载两套语言 | 发布包实测 |
| L16 | 双语 README 关键项目条目、链接与命令 | 内容同步、链接可用、无虚构功能 | 文档审查 |

测试设备不具备时，写 `NOT TESTED: <reason>`；不允许将“编译通过”等同于“语言/朗读验收通过”。

至少保存两种语言界面截图（设置页面、规则编辑页面和有无障碍反馈的页面），记录 Windows 构建、系统语言、Narrator 是否安装对应声音、安装形式、资源加载方式与测试 commit。

## 10. 开发完成时 CodeX 必须回报

新建 `docs/releases/patches/RC-01-WinUI3-i18n-en-US-verification.md`，结构：

1. **Git 基线**：start/end commit、branch、dirty files、是否实际合并。
2. **改动文件列表**：资源、代码、测试、README、构建脚本、相关文档；变更理由。
3. **语言解析/配置设计**：系统模式、强制模式、持久化、SDK/打包差异。
4. **屏幕阅读器与无障碍**：测试手段、读取的样例文字、是否真正切换 TTS 声音、限制。
5. **测试结果**：命令、PASS/FAIL/NOT TESTED、机器环境、真实发布包资源验证。
6. **已有 Smoke 对比**：当前新 commit / SHA、受影响验证项，不能继承旧版本 PASS 冒充新版本测试。
7. **剩余阻断问题与非阻断问题**：严重度、复现、建议。
8. **最终结论**：`READY FOR RC SMOKE` 或 `BLOCKED`（只能在证据允许时标 `READY FOR RELEASE`）。

不得自动 push、创建远程 Release、删除分支、覆盖既有测试记录。用户将人工核查之后继续 RC。

## 11. 可复制给 CodeX 的执行指令

> 请实施 `docs/releases/patches/RC-01-WinUI3-i18n-en-US.md`。先检查实际 Git 基线、AGENTS.md、当前 RC smoke 状态与 WinUI 3 分发方式，不得把本文误认为已经应用的代码补丁。使用 WinUI 3 原生 `.resw` 完成 en-US 和 zh-CN 双语言 UI；语言选择包括 system/en-US/zh-CN、自动匹配 Windows 系统语言并持久化，切换后重开 Settings 生效，agent 保持运行。使所有 Narrator/UI Automation 用户可读文本跟随所选 UI 语言；不试图替用户修改系统朗读语音。更新仓库根英文 README.md 与中文 README.zh-CN.md，确保两者基于最新代码且互链。重点验证 unpackaged/packaged 资源加载与最终 publish/installer，重新执行相关 RC Smoke。不得改动 Rust remapping 语义、规则 schema、协议或已有历史验收结果。记录实际文件、测试命令/结果、未验证项于 `docs/releases/patches/RC-01-WinUI3-i18n-en-US-verification.md`；不进行远端发布。

## 12. 权威技术文档（实施过程中按当前 SDK 版本核对）

- WinUI 3 `.resw` + `x:Uid` + `ResourceLoader`、unpackaged `.pri` 的注意事项：<https://learn.microsoft.com/en-us/windows/apps/winui/winui3/localize-winui3-app>
- Windows App SDK `ApplicationLanguages.PrimaryLanguageOverride` 的进程初始化、持久化与动态刷新：<https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.windows.globalization.applicationlanguages.primarylanguageoverride>
- WinUI 3 `AutomationProperties.Name` 的资源化：<https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.automation.automationproperties.name>
- WinUI 3 `FrameworkElement.Language`：<https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.frameworkelement.language>
- Windows Narrator 多语言朗读与语音安装约束：<https://support.microsoft.com/en-us/accessibility/windows/narrator/chapter-4-reading-text>

---

**最终原则**：本补丁是为了让首个公开 RC 更易使用，而不是扩大 InputFlow 的核心功能范围。功能变更越靠近发布，越需要用新的真实 Smoke 证据来证明它没有破坏原有输入、设置保存和发布流程。
