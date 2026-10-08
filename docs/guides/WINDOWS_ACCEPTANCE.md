# InputFlow Windows 实机验收工具

> 文档类型：验收指南
>
> 工具目录：`scripts/acceptance/`

`scripts/acceptance/` 为 Phase E、M6 和 Phase F 实机验收提供可重复的观察工具，不会伪造“物理输入”证据。结论必须区分自动测试、脚本注入和用户实际操作。

## 工具

- `scripts/acceptance/message-target.ps1`：显示一个真实 Win32/WinForms 目标窗口，顺序记录键盘 down/up/char、鼠标按钮、`WM_CONTEXTMENU`、光标位置、HKL 和 DPI。
- `scripts/acceptance/sample-agent-resources.ps1`：经过真实 Named Pipe 每隔一段时间调用 `get_stats`，同时记录 agent/设置进程的内存、线程、句柄和 CPU。
- `scripts/acceptance/invoke-agent-request.ps1`：在可选延时后通过真实 Named Pipe 发送 `pause` / `resume` / `get_status` / `get_stats`，用于可重复的暂停交错验收。
- `scripts/acceptance/pause-on-foreground-transition.ps1`：先观察指定普通目标成为前台，再在提升权限目标成为前台的瞬间通过正式 Named Pipe 发送 `pause`；用于避免人工倒计时掩盖 UIPI 回放边界。
- `scripts/acceptance/configs/*.json`：M6 四类触发器的单规则隔离配置，以及 Phase F 的
  `mouse-direction-f8-four.json` 四方向组；命中后只输出既有键盘组合。

## 证据目录

每次会话在 `target/acceptance/<timestamp>/` 保存运行证据：

- `target-messages.log`：目标窗口的真实消息顺序；
- `resources.csv`：资源和 `get_stats` 延迟；
- `config-before.json` / `config-after.json`：UI 编辑前后的配置读回；
- `agent-log-tail.txt`：本轮 agent 日志片段；
- `observations.md`：物理手势、用户可见现象、通过/失败/未执行。

## 分阶段人工操作

1. Phase E capture：在空配置上依次录制 Caps Lock、OEM 键、方向键、主 Enter、keypad Enter 和媒体键，保存后关闭/重开设置并读回。缺失实体键必须记为“硬件不可用/未实测”。
2. Phase E 编辑：用 UI 分别创建 hold、key+mouse、hold+mouse，逐条保存、在目标窗口物理触发，然后验证启停、删除、F12 与托盘/UI 状态同步。
3. M6 触发器矩阵：每次只加载一个 `configs/*.json`，执行命中、前缀失败、自动重复、暂停交错和待定队列溢出；对照 `target-messages.log` 和 agent stats。
4. M6 权限/输出：先用普通目标窗口观察完整插入，再由用户确认 UAC 启动提升权限目标，观察 UIPI 零插入。部分插入若无法稳定复现，保留故障注入证据并明确写“实机未触发”。
5. 高负载/长稳态：资源采样运行 5 分钟期间持续物理打字、点击和移动鼠标。记录样本数、p50/p95/p99/max、stats RTT、Hook 存活和资源趋势。
6. 可访问性：用键盘遍历全流程，开启 Narrator 读取名称/状态，分别在高对比度与测试机实际缩放档位检查剪裁、溢出和焦点；本轮为 125%/150%/恢复 200%。缺少对应语言的辅助语音时，另用 UIA 名称枚举取证并明确记录语音环境限制。
7. 关闭边界：使用可控制的 agent/probe 退出，分别在仍按住已消费键/右键时于 2 秒内释放、超过 2 秒再释放，检查残留 Up 与退出日志。
8. Explorer：经用户同意后真实重启 Explorer，确认托盘图标恢复、菜单仍可用、agent PID 不变。
9. Phase F 方向：加载 `mouse-direction-f8-four.json`，分别完成四方向、距离不足、偏轴、超时、同次按住继续移动、F12／pause、普通拖拽和五分钟混合 move；实际物理手势不得由脚本注入代替。

## 建议判定线

这些是本轮验收线，不是对 Windows API 最坏时间的产品保证：`get_stats` p99 < 20 ms，Hook callback p99 < 5 ms、max < 100 ms，五分钟内无 Hook 丢失、无粘键/鼠标卡住、无 output failed/dropped，agent 和设置进程资源无持续单调增长。
