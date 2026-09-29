# ADR-005：逻辑键、物理键身份与 Schema v2

- 状态：已接受（Accepted）
- 日期：2026-09-29
- 涉及模块：`inputflow-engine`、`inputflow-windows`、`inputflow-config`、未来 agent IPC 与 WinUI 设置程序

## 背景（Context）

原有 `Key` 只覆盖修饰键、字母、数字、功能键和少量常用键；规则只保存字符串键名。低级键盘 Hook 实际同时给出 `vkCode`、`scanCode` 和 `LLKHF_EXTENDED`，但旧模型没有声明规则要跟随“逻辑键”还是“物理位置”。这会在 OEM 符号、左右修饰键、主 Enter/小键盘 Enter、Num Lock 状态和键盘布局变化时产生歧义。

本阶段还必须保留 M6 的同步抑制、FIFO 回放、repeat、consumed-release tombstone、pause/overflow/quit 串行顺序和 `SendInput` 失败旁路语义。身份扩展不能在 Hook 中增加 IPC、磁盘访问或无界分配。

## 候选方案（Options）

### 方案 A：扩展枚举，继续只按 VK 匹配

- 配置和匹配最简单，已有 v1 字符串可直接扩展。
- 字母、数字、媒体键和大多数导航键的逻辑语义清楚。
- OEM VK 的标签和产生字符会随布局变化；小键盘在 Num Lock 关闭时也可能以导航 VK 出现。
- 无法表达“无论布局如何都匹配这一物理位置”，也无法用同一个稳定逻辑名字无损表示所有厂商键。

### 方案 B：只按 scan code + extended 匹配

- 对键盘位置和左右/扩展变体最精确；布局切换不会改变规则位置。
- 对“复制”“媒体播放”“Caps Lock”等语义型规则不友好；换键盘、远程输入、软件注入或某些无 scan code 的媒体/厂商输入可能失效。
- 配置不可读，旧规则迁移时只能猜测当前布局，迁移成本和错误风险高。

### 方案 C：事件同时保留逻辑与物理身份，规则显式声明 match mode

- Hook 事件保留逻辑 VK，以及 scan code + extended；规则选择 `logical` 或 `physical`。
- 语义型快捷键使用逻辑身份，游戏式/位置型规则使用物理身份。
- 配置稍复杂，UI 必须显示默认选择并允许高级切换；Rust/C# DTO 需要共同契约。

## 取舍（Trade-offs）

| 维度 | 仅 VK | 仅 scan | 双身份 + match mode |
|---|---|---|---|
| 输入正确性 | 布局/小键盘位置有歧义 | 语义和设备迁移有歧义 | 由规则作者明确选择 |
| Hook 耗时 | 最低 | 最低 | 只复制两个整数和标志，仍为常数时间 |
| 内存 | 小 | 小 | 每个事件增加的字段已有来源且为定长；pending 仍有界 |
| 跨线程顺序 | 不变 | 不变 | 不新增线程或异步确认，仍由 Hook owner 串行 |
| 崩溃/回放 | VK 回放可能随布局改变 | scan 回放保位置 | 捕获回放保留原 scan；配置动作依 match mode 输出 |
| v1 兼容 | 最简单 | 需要猜测 | v1 字符串确定迁移为 logical，不猜测 |

选择方案 C。它是唯一同时避免布局猜测、保留旧语义并允许用户表达位置意图的方案。

## 最终决策（Decision）

### 1. 身份模型和优先级

- 每个键盘事件保存逻辑 `Key`、`scan_code` 和 `extended`。
- v2 规则键必须是以下两种互斥结构之一：

```json
{ "match": "logical", "key": "CapsLock" }
{ "match": "physical", "scan_code": 58, "extended": false }
```

- `scan_code == 0` 不能成为 physical 配置身份。
- 同一事件同时命中 exact physical 和 logical 前缀时，physical 规则优先。这使显式位置规则不会被较宽泛的逻辑规则遮蔽。
- held/repeat/release tombstone 的内部 tracking key 优先使用 physical 身份；没有 scan code 时才退回 logical。即使按住期间布局改变，物理 release 仍能清除正确墓碑。
- 紧急旁路键只允许 logical 身份并在 matcher 前处理；它始终优先于规则。跨 logical/physical 的静态冲突无法仅由平台无关配置层完全判断，未来 agent/UI 应提示同一实键可能使 physical 规则不可达。

### 2. 键的区分

- 字母和主键盘数字以稳定 VK 名称表示，如 `A`、`Digit1`；physical 模式另存实际 scan。
- OEM 符号使用 Windows VK 身份 `Oem1`、`OemPlus`、`OemComma`、`OemMinus`、`OemPeriod`、`Oem2`…`Oem8`、`Oem102`，不把 US 布局的 `;`、`[` 等字符当稳定 identity。
- 左右 Ctrl/Alt 根据 VK 或 extended 区分；左右 Shift 在通用 `VK_SHIFT` 时根据 scan `0x2A/0x36` 区分。
- 主 Enter 与 keypad Enter 共享 `VK_RETURN`，由 extended 位区分为 `Enter` / `NumpadEnter`。
- `Numpad0`–`Numpad9` 及运算键有独立逻辑身份；Num Lock 关闭时 Windows 可能报告导航逻辑身份，要求位置不变的规则应使用 physical。
- 锁定键、导航键、PrintScreen、Pause、Apps/Menu、音量、媒体和浏览器键均有具名逻辑身份。

### 3. UI 默认和布局相关显示名

录制一个键后默认建立 **logical** 规则，因为普通快捷键通常表达“Caps Lock”“播放/暂停”“Ctrl+C”等语义，也与 v1 一致。高级设置允许切换为 physical，并明确说明它跟随位置而非字符。

配置只保存上述稳定 identity，不保存本地化显示名。UI 在显示时按当前 `HKL` 动态计算标签：

1. physical identity 直接把 scan code 和 extended 组成 `GetKeyNameTextW` 所需的 `lParam`；
2. logical identity 先用当前 `GetKeyboardLayout` 与 `MapVirtualKeyExW(MAPVK_VK_TO_VSC_EX)` 获取 scan/扩展信息，再调用 `GetKeyNameTextW`；
3. 对可打印 OEM 键，可用 `ToUnicodeEx` 生成当前布局的辅助字符预览，但必须使用独立、不会污染真实死键状态的查询策略；失败时显示 Windows 键名和稳定 `Oem*` identity。

显示名会随语言、布局和系统本地化变化，因此不能作为协议键值、冲突键或迁移依据。设置录制只观察候选事件，不暂停、替换或启用规则，也不得把锁定键录制误当运行控制；录制协议在 Phase D/E 接线。

### 4. 输出与回放

- logical 配置动作使用 VK 模式：`KEYBDINPUT.wVk`，必要时加 `KEYEVENTF_EXTENDEDKEY`。
- physical 配置动作使用 scan 模式：`wVk = 0`、`wScan = scan_code`、`KEYEVENTF_SCANCODE`，按 identity 设置 extended。
- 被暂存的真实事件优先按其原始 scan/extended 回放；只有 scan 为 0 才退回观察到的 VK。down/up 原样映射，up 另加 `KEYEVENTF_KEYUP`。
- PrintScreen、keypad Enter、导航、右 Ctrl/Alt、Apps 和部分媒体键必须保留 extended。
- 字符/文本输出不是物理按键输出。未来若增加文本动作，应单独设计 Unicode/text action，不得把 `KEYEVENTF_UNICODE` 混作 physical 或 logical key action。
- 所有输出继续携带 InputFlow 自有 `dwExtraInfo`，回到 Hook 后直接放行，不能参与 repeat 或匹配状态。

### 5. Unknown 和厂商键

- 无已知 VK 的 Hook 事件保留为 `Unknown(raw_vk)`，不静默映射成 `Oem*` 或其他键。
- Unknown 事件仍可普通放行；失败回放时若有 scan code 使用原 scan，否则使用原始 VK，尽量保真。
- schema v2 拒绝 `Unknown` logical 配置名。用户只有在观察到非零 scan 后才能显式保存 physical identity；scan 也为零的厂商键在有可靠模型前拒绝配置。

### 6. Schema v1 兼容、迁移和回滚

- v1 的字符串键全部确定迁移为 v2 `logical`，包括既有 `LeftCtrl`、`C` 等；不根据当前布局猜 physical。
- loader 严格区分 v1/v2，拒绝未知字段、混合键形状、未知 logical 名、scan 0、physical 紧急键和不支持版本。
- v1 只在内存中迁移；读取本身不覆盖用户文件，并返回 compatibility warning。
- 所有新保存只写 v2。原子替换的 committed backup 保留原 v1，可用于回滚；迁移保存失败时正式文件不先被删除。
- `fixtures/config/v1-valid.json` 与 `v2-valid.json` 是未来 Rust/C# DTO 的跨语言 golden contract。C# 在能往返这些 fixture 前不得成为配置写入方；正式保存仍由 agent 权威校验。

## 行为、线程、事件序列与失败模型

正常匹配序列：Hook owner 同步取得 `{vk, scan, extended}` → 常数时间归一化 → matcher 依 physical-first 候选查找 → PassThrough 或 Suppress → 若匹配/失败则在同一 owner 线程同步 `SendInput` → 自有注入事件放行。

锁定键失败序列：Caps down 被暂存并抑制 → 候选失败/提前 up → FIFO 只生成原始 down/up 一次 → scan-code `SendInput` → 自有注入标志防止递归。因此成功回放应只产生一次正常 toggle，不补造额外 down。

pause、F12、overflow 和正常 quit：都必须在 Hook owner 顺序点取走 pending FIFO；未消费的 Caps down/repeat 按原顺序回放，已经成功消费的 down 绝不复活，只留下等待 physical up 的 tombstone。overflow 或 `SendInput` 0/部分成功后进入旁路并记录诊断；不得声称已经恢复无法确认的先前输入。强杀仍无法保证回放，这是既有明确限制。

身份扩展没有新增 worker、锁、IPC 或无界容器。Key 是定长值；候选最多检查 physical 和 logical 两项。布局显示名查询只能在设置/诊断非 Hook 路径执行。

## 实验 / 依据（Evidence）

- `Key::NAMED` 全集合通过 VK → Hook mapping 往返测试，覆盖最低键集和左右/extended 特例。
- engine 测试覆盖 physical 优先级、布局变化后的 release tombstone、Caps 无规则、失败、命中、repeat、pause 和 overflow。
- Windows 平台测试验证捕获 Caps down/up 生成一对 scan-code INPUT，且 up 方向正确；logical/physical action 分别选择 VK/SCANCODE。
- config golden 测试验证 v1 迁移、v2 logical/physical 往返、非法组合拒绝、v2 保存及 v1 committed backup 回滚。
- US 与用户常用布局的真实符号键录制/显示/回放，以及 Caps 指示灯/目标窗口结果仍必须按 Windows 人工矩阵记录；自动测试或 `SendInput` 注入不能冒充真实物理输入证据。

## 后果（Consequences）

### 正面

- 语义快捷键和物理位置规则都可明确表达，且旧配置含义不变。
- 捕获回放不会因保存时/回放时的布局差异改用错误 VK。
- 锁定键的 down/up 数量可以在纯测试中审计，降低重复 toggle 风险。
- future C# DTO 有可执行的 golden contract。

### 成本与后续

- UI 必须同时显示稳定 identity、当前布局标签和 match mode，不能只显示字符。
- 非零 scan 并不等同于跨所有硬件/远程环境的永久设备 identity；physical 是 Windows scan 位置语义，不承诺具体设备来源。
- Phase D/E 仍需实现有界、显式、可取消的录制协议以及非 Hook 路径的动态显示名服务。
- Phase B 的“完整键盘支持完成”声明仍受 US/用户常用布局与 Caps Lock 真实物理验收证据约束。

## 官方依据

- KBDLLHOOKSTRUCT：https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct
- Virtual-Key Codes：https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes
- Keyboard Input Overview：https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input
- GetKeyNameTextW：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getkeynametextw
- MapVirtualKeyExW：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-mapvirtualkeyexw
- ToUnicodeEx：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-tounicodeex
- KEYBDINPUT：https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput
- SendInput：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
