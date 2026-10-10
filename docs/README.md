# InputFlow 文档索引

> 文档状态：当前索引
> 最后更新：2026-10-10（Australia/Brisbane）

本文是仓库文档的唯一入口。项目的详细进度只记录在
[`status/CURRENT_STATUS.md`](status/CURRENT_STATUS.md)；历史文件不能作为当前状态依据。

## 当前阅读顺序

1. [`status/CURRENT_STATUS.md`](status/CURRENT_STATUS.md)：当前阶段、已验证证据、未完成项和下一步。
2. [`tasks/FIRST_RELEASE.md`](tasks/FIRST_RELEASE.md)：当前唯一正在执行的任务卡，当前阶段为 RC。
3. [`records/FIRST_RELEASE_RC_EXECUTION.md`](records/FIRST_RELEASE_RC_EXECUTION.md)：`v0.9.0` RC 的版本、构建、包与 smoke 证据。
4. [`releases/patches/RC-01-WinUI3-i18n-en-US-verification.md`](releases/patches/RC-01-WinUI3-i18n-en-US-verification.md)：英语／国际化补丁的本地实施证据与新 RC 待测项。
5. [`records/FIRST_RELEASE_H_EXECUTION.md`](records/FIRST_RELEASE_H_EXECUTION.md)：H 分发与生命周期完成证据。
6. [`releases/V0.9.0.md`](releases/V0.9.0.md)：当前版本的用户发布说明草稿。
7. [`archive/phase-f/PHASE_F.md`](archive/phase-f/PHASE_F.md)：Phase F 的完整完成记录与限制。
8. [`planning/PROJECT_PLAN.md`](planning/PROJECT_PLAN.md)：产品范围、架构和不变量。
9. [`planning/RELEASE_ROADMAP.md`](planning/RELEASE_ROADMAP.md)：首版发布阶段顺序和门槛。
10. [`guides/BUILD_WINDOWS.md`](guides/BUILD_WINDOWS.md)：Windows 构建与验证命令。
11. [`guides/USER_GUIDE.en-US.md`](guides/USER_GUIDE.en-US.md)／[`guides/USER_GUIDE.md`](guides/USER_GUIDE.md)：英语／中文用户指南。
12. [`decisions/`](decisions/)：已接受的架构决策。

## 目录职责

| 目录 | 内容 | 是否记录当前进度 |
|---|---|---|
| `governance/` | 文档存储、命名、生命周期和维护规则 | 否 |
| `planning/` | 稳定的项目范围、路线和执行顺序 | 只链接当前状态 |
| `status/` | 跨项目的当前进度 | **是，唯一权威** |
| `tasks/` | 尚未完成或尚未开始的可执行任务卡 | 只描述门槛，不复制总进度 |
| `records/` | 当前阶段的命令、测试和现场证据 | 只记录本阶段事实 |
| `guides/` | 可重复的构建、运行和验收流程 | 否 |
| `releases/` | 版本对应的用户发布说明和下载／支持信息 | 否 |
| `reference/` | 术语、研究日志和 fixture 契约说明 | 否 |
| `decisions/` | 仍然有效的 ADR | 否 |
| `archive/` | 已完成任务、旧计划和历史快照 | **否** |

详细规则见
[`governance/DOCUMENTATION_POLICY.md`](governance/DOCUMENTATION_POLICY.md)。

## 后续任务

- 当前按 [`tasks/FIRST_RELEASE.md`](tasks/FIRST_RELEASE.md) 执行 `v0.9.0` RC。
- 首版实际发布后，进入 [`tasks/POST_RELEASE_SOAK.md`](tasks/POST_RELEASE_SOAK.md)。
- 完成的任务卡和记录必须移动到 `archive/`，不得继续留在 `tasks/` 或 `records/`。
