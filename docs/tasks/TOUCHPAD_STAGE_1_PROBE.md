# InputFlow · Stage 1 触控板只读探针

对应[触控板扩展提案](../planning/InputFlow-Touchpad-Expansion-Proposal.md)的 Stage 1。本次验收范围为一台 Windows 笔记本的前台 Raw Input。

## 要回答的问题

手指轻放在触控板上时，普通权限的独立程序能否观察到接触开始、接触结束、触控板表面的位置和经过时间？如果光标停住，程序能否分清“手指仍在原地”和“手指已经抬起”？这是后续轻点左键、长触右键的共同前提。

## 第一次交给 Codex 的工作

先读触控板提案第 15 节、仓库 Cargo.toml 和适用的 AGENTS.md。先实现下文 1A，在本地 Windows 构建并给出一条实机运行命令。等拿到报告计数和来源后再选 1B 的解析路径。如果 Codex 与操作者共享 Windows 主机，它可以自行完成设备查询、编译、启动和汇总，操作者只需按提示操作触控板。

建议在 `experiments/touchpad-probe/` 新建 Rust 二进制 crate，使用仓库已采用的 windows-sys 0.61.2。独立 manifest 声明自己的 `[workspace]`，再按所需 API 添加 features。可用以下命令单独构建运行。

```powershell
cargo run --manifest-path .\experiments\touchpad-probe\Cargo.toml
```

只注册触控板用途，在普通用户权限下观察。阶段一不注入鼠标输入，不装驱动，不改 Windows 设置。Agent、现有 probe-cli、WinUI、规则配置与发布包均不需要参与。

## 1A · 看见设备与报告

**本机前台验证通过**。已识别触控板集合、成功注册并读取可归因的 HID 报告。[1A 实测记录](../records/TOUCHPAD_STAGE_1A_PROBE.md)

1. 记下 Windows build。先用 Windows 的 PnP 设备清单筛查触控板用途。有结果只是设备线索，空结果也要继续尝试 Raw Input 注册。

   ```powershell
   Get-CimInstance Win32_OperatingSystem | Select-Object Caption, BuildNumber
   Get-CimInstance Win32_PnPEntity |
       Where-Object { $_.HardwareID -contains 'HID_DEVICE_UP:000D_U:0005' } |
       Select-Object Name, Status
   ```

2. 探针用 GetRawInputDeviceList / GetRawInputDeviceInfo 枚举候选集合，按 usage page 0x0D、usage 0x05 识别触控板。记录枚举到的候选数和 API 错误。使用可置于前台的普通 Win32 窗口、消息循环和 RegisterRawInputDevices 订阅该用途。注册成功后，在 WM_INPUT 中用 GetRawInputData 读取消息。
3. 显示注册结果、收到的消息数、输入类型、HID 报告数。RAWHID 可能一次携带多份报告，按 dwSizeHid 和 dwCount 检查边界并计数。hDevice 有可能为零，单列其数量；无法确认来源或取得描述信息时保留“不确定”。无需注册普通键盘或鼠标。
4. 操作者让探针窗口处于前台，依次空闲、单指静止触碰并抬起、单指缓慢滑动、只移动外接鼠标（如有）。按动作分段记录计数变化。需要了解全局可行性时，用明确启用 RIDEV_INPUTSINK 的实验再测一次切到记事本的情况，单独记录。
5. 若成功注册但没有可归因的 HID 报告，先核对窗口焦点、注册返回值、消息循环、消息类型以及空句柄分支。仍无法取得时，记录“此设备和此读取路径尚未提供可确认的报告”，交付诊断结果。看到光标移动不能作为读取到单指接触的证据。

## 1B · 从报告还原一根手指

**本机前台单指接触解码通过**。已在短触、静止触碰和慢滑中验证单指接触起止、设备坐标及观测时长。[1B 实测记录](../records/TOUCHPAD_STAGE_1B_PROBE.md)

当 1A 取得可归因的报告后，利用设备的 preparsed data 与 HidP 系列函数，按实际描述符、报告 ID 和联系同一根手指的 link collection 解析 Tip Switch、Contact ID、X、Y。检查设备的坐标逻辑范围，输出原始设备坐标或明确标注的归一化值。坐标不取自 GetCursorPos。若现有数据无法可靠关联到描述符，报告解析受阻的具体位置。

用 Tip 的变化识别接触和抬起，按设备和 contact ID 追踪一次接触；同一 ID 抬起后可再次使用。用单调时钟计算观测到的持续时间。静止期间不要求每隔固定时间都有新报告，不把短暂沉默当成抬起。将报告批次、无关报告和多指情况与单指生命周期区分开。控制台优先显示转变与每次接触的摘要。

## 实机验收

建议每项先试 5 次，记实际次数和异常；数字只是帮助发现偶发漏报。完成后顺手确认 Windows 原有移动、轻点和双指滚动仍然正常。

| 操作 | 看什么 |
|---|---|
| 在原地轻碰后立即抬起 | 每次有一组 down/up。即便光标没动也能看到 |
| 手指轻放约一秒再抬起，不压下触控板 | 一次 down、一次 up，记录时长接近实际。静止期间没有虚假抬起 |
| 从左向右滑，再从上向下滑 | 同一接触的位置和移动方向一致，抬起结束；观察中途是否换 ID 或断开 |
| 分别反复轻触左侧和右侧固定位置 | 两组 X 值可区分，在相同位置重复触碰结果相近 |
| 如果有外接鼠标，仅移动鼠标 | 探针没有把外接鼠标动作识别为触控板接触 |

有实测之后，将机器型号、Windows build、探针版本、每项次数、失败现象和可复现命令记在 `docs/records/`。写明是前台还是后台。最终结论区分“可识别设备”“可读取报告”“可还原单指生命周期”；只有最后一项连同坐标、时长及原生操作验收通过，才进入 Touch Click 实验。

## 实现时核对的微软资料

- [Windows Precision Touchpad Collection](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-windows-precision-touchpad-collection)
- [Windows 系统打开的 HID 顶层集合与访问模式](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/top-level-collections-opened-by-windows-for-system-use)
- [Using Raw Input](https://learn.microsoft.com/en-us/windows/win32/inputdev/using-raw-input)
- [RAWINPUTHEADER 的 hDevice 说明](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputheader)
- [Interpreting HID reports](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/interpreting-hid-reports)
- [GetPointerTouchpadInfo 的手势投递限制](https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/getpointertouchpadinfo)
