# InputFlow

[English](README.md) | [简体中文](README.zh-CN.md)

InputFlow 是 Windows 全局键盘与鼠标输入组合引擎。它只暂扣可能构成已启用规则的事件；命中后消费输入
并发送动作，失败或超时则按序尽力回放。

## 当前状态

InputFlow v0.9.0 正在准备首个公开 Pre-release。英语补丁前的 RC 包已通过自动、桌面与物理输入 smoke；
当前 RC-01 补丁为 WinUI Settings 增加完整英语和简体中文资源，发布前必须重新生成包、固定 SHA-256 并
完成受影响 RC smoke。远端 release 尚未发布。权威进度见[当前状态](docs/status/CURRENT_STATUS.md)。

## 架构与安全边界

- `inputflow-agent.exe` 是唯一的 Hook、托盘、规则运行时、正式配置和 IPC 服务所有者。
- `InputFlow.Settings.exe` 是按需启动的 C# + WinUI 3 设置程序；关闭最后一个窗口只退出 Settings。
- Settings 不安装 Hook、不直接写正式配置；Agent 负责验证、原子保存和热应用。
- Agent 不加载 .NET、WinUI、WebView 或 JavaScript 运行时。
- Hook 热路径不执行 UI、磁盘、网络、无界分配、无界队列或等待 UI。
- `F12` 保留为紧急旁路；输入录制不能吞掉它。
- 项目不使用 Tauri、React、Node.js、npm、WebView2 或 Electron。

## 已实现能力

- KeyChord、Key+MouseButton、Hold、Hold+MouseButton 和鼠标四方向触发器。
- logical／physical 键身份、完整具名键、scan-code 回放和布局相关显示名。
- 键盘组合动作、持久化规则启停、配置验证、原子保存、恢复和运行时热替换。
- Rust/Win32 常驻 Agent、托盘、单实例和 Explorer 托盘恢复。
- 版本化、有限帧、当前用户 ACL 的 Windows Named Pipe。
- WinUI 规则编辑、录制、连接协调、诊断和可访问性支持。
- Schema v4 鼠标方向参数：激活键、净位移阈值、偏轴容差、超时和一次命中；普通 move 始终直通。
  Schema v1／v2／v3 保持可读并确定迁移。

## 安装并运行便携测试版

发布包名为 `InputFlow-0.9.0-win-x64.zip`，面向 Windows x64，声明最低 Windows 10 build 17763。
unpackaged 目录随包携带 self-contained .NET 与 Windows App SDK；系统仍需 Microsoft Visual C++
Redistributable 2015–2022 x64。首版未签名。

1. 使用随附的 `SHA256SUMS.txt` 核对 zip。
2. 将完整压缩包解压到普通用户可写目录，保留所有 DLL、PRI、说明和许可文件。
3. 运行 `inputflow-agent.exe`，再从托盘图标打开 Settings。
4. 使用 `F12` 紧急暂停或恢复；需要退出时从托盘正常退出 Agent。

RC-01 重新构建并验收前，新的精确公开附件尚不存在，因此本文不虚构下载地址。完整操作、升级、自启动
和移除说明见[中文用户指南](docs/guides/USER_GUIDE.md)。

## 显示语言与辅助功能

Settings 默认跟随 Windows 显示语言，无匹配时回退英语。可在 **设置与诊断 > 显示语言** 选择
**System default / 跟随系统**、**English** 或 **简体中文**，然后关闭并重新打开 Settings。Agent 与
当前规则会继续运行。

`en-US` 与 `zh-CN` 的可见文本、动态通知和 UI Automation 名称均本地化。Narrator 会读取所选界面
语言的标签；实际音色取决于 Windows 已安装的语言语音。InputFlow 不安装语音，也不修改系统 Narrator 设置。

## 构建与测试

工具链和准确前置条件见 [Windows 构建指南](docs/guides/BUILD_WINDOWS.md)。

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p inputflow-agent --release
./scripts/build-windows.ps1
```

完整脚本验证 Rust、协议与 Settings Core runners、WinUI Debug／Release 构建、资源键一致性，以及英语／
中文运行时资源装载。`scripts/package-release.ps1` 负责单独打包；RC 产物必须来自固定的干净提交。

## 已知限制

- 真实跨屏、跨 DPI 和热插拔尚未在代表性硬件完成验证。
- `SendInput` 受 UIPI、焦点和修饰键状态影响，不能保证对所有目标完整回放。
- 进程被强杀时，已暂扣的历史输入无法保证恢复。
- 特殊键硬件、partial `SendInput`、睡眠／唤醒和 Narrator 实际音色仍依赖环境或未完整验证。
- 公开测试版不宣称已完成 24／72 小时长测。
- Fn、任意三键、按应用生效及 URL／程序／文件夹动作尚未实现。

## 文档与反馈

- [文档索引](docs/README.md)
- [当前状态](docs/status/CURRENT_STATUS.md)
- [首版发布任务](docs/tasks/FIRST_RELEASE.md)
- [RC 执行记录](docs/records/FIRST_RELEASE_RC_EXECUTION.md)
- [Windows 构建指南](docs/guides/BUILD_WINDOWS.md)

反馈问题时请提供 InputFlow 版本、Windows build、规则类型与参数、复现步骤、目标程序是否提升，以及移除
敏感数据后的相关日志。不要上传私人配置或完整输入轨迹。

## 许可证

InputFlow 采用 [MIT License](LICENSE)。第三方组件及其许可证见
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt)；发布包还包含锁定依赖的完整许可文件。
