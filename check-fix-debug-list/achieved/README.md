# 历史项目检查清单

> [!CAUTION]
> 本目录中的文件是 InputFlow 仍采用 Tauri 前端方案时生成的历史任务和修复记录。
> 这些文件仅用于问题追溯，不得作为当前架构或开发任务的实施依据。

InputFlow 当前已经采用：

- 纯 Rust + Win32 的常驻 agent；
- C# + WinUI 3 的按需设置程序；
- 版本化 Windows Named Pipe；
- 关闭设置窗口后仅 agent 常驻。

当前开发必须以以下文件为准：

- [M6 可靠性基线摘要](../M6-可靠性基线摘要.md)
- [M7/M8 当前任务书](../tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md)
- [ADR-004：Rust Agent 与 WinUI 3](../../docs/decisions/ADR-004-Rust常驻Agent与WinUI3设置程序.md)
- [项目规划](../../docs/PROJECT_PLAN.md)

本目录中的 Tauri、React、Node.js、npm、WebView 或旧 `apps/desktop`
相关内容均已失效。

当前清单已经提炼旧文档中仍然有效的可靠性结论；
旧文档中的详细调查过程仅供历史追溯。