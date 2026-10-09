# InputFlow 文档存储与生命周期规则

> 文档类型：治理规则
>
> 状态：已采用
>
> 生效日期：2026-10-08（Australia/Brisbane）

## 1. 存储边界

- 仓库根目录只保留一个 Markdown 文件：`README.md`。
- 其他 Markdown 文档一律存放在 `docs/` 的职责目录中。
- 源码、fixture 和脚本目录不放说明性 Markdown；对应说明分别放入 `docs/reference/` 或
  `docs/guides/`，并使用仓库根相对路径指向实际文件。
- 不再创建 `check-fix-debug-list/`、根目录交接文档或同一内容的双份规划。

## 2. 目录职责

| 路径 | 允许内容 |
|---|---|
| `docs/governance/` | 文档规则、贡献约定和维护流程 |
| `docs/planning/` | 产品规划、发布路线和稳定的执行顺序 |
| `docs/status/` | 唯一的跨阶段当前状态 |
| `docs/tasks/` | 未完成任务的验收条件和操作清单 |
| `docs/records/` | 当前阶段实际执行的证据，不放未来阶段模板 |
| `docs/guides/` | 构建、运行、验收和故障排查步骤 |
| `docs/releases/` | 版本对应的用户发布说明、下载和已知限制 |
| `docs/reference/` | 术语、研究和协议／配置 fixture 说明 |
| `docs/decisions/` | ADR 模板和仍有效的决策 |
| `docs/archive/<milestone>/` | 已完成任务、旧计划、交接和历史执行快照 |

## 3. 进度的唯一权威

[`../status/CURRENT_STATUS.md`](../status/CURRENT_STATUS.md) 是详细项目进度的唯一权威。

- 根 `README.md` 只给一段摘要并链接到当前状态。
- planning、tasks 和 guides 不复制测试数量、HEAD、阶段状态表或“下一步”段落。
- 当前阶段的测试命令和证据写入 `records/`；`CURRENT_STATUS.md` 只引用其结论。
- 同一状态发生变化时，先更新当前记录，再更新 `CURRENT_STATUS.md` 和根 README 摘要。
- 历史文档不追随当前进度更新；它们必须位于 `archive/`，并明确标为历史快照。

发生冲突时，优先级为：当前用户指令 → 适用的 `AGENTS.md` → 已接受 ADR →
`CURRENT_STATUS.md` → 当前任务卡 → 规划和指南 → archive。

## 4. 文件生命周期

1. 尚未开始但已确定范围的工作放在 `tasks/`。
2. 开始执行时，在 `records/` 建立单阶段记录；不要建立跨多个未来阶段的空模板。
3. 执行期间，任务卡描述验收门槛，记录文件保存实际证据，当前状态只保存汇总结论。
4. 阶段完成后，将任务卡和记录移动到对应 `archive/<milestone>/`。
5. 被新文档替代的旧计划、交接说明和审查清单直接归档；完全相同的副本删除，不再保留第二份。
6. archive 中的错误事实可以更正，但不能把历史快照改写成当前任务入口。

## 5. 命名与链接

- 新文件使用稳定、描述性的英文大写蛇形名称，例如 `CURRENT_STATUS.md`、
  `FIRST_RELEASE.md`。既有 ADR 文件名保持不变，以免破坏决策编号。
- 名称不使用 `tag_1`、`tag_5.1`、`final-final` 等依赖排序或会失去语义的前缀。
- 文档内链接使用相对路径；命令和表格中的实际仓库路径使用根相对路径。
- 移动文件时必须在同一改动中更新所有非历史引用。
- 每份活动文档应在标题后说明文档类型、状态或适用范围；archive 文档必须显示历史提示。

## 6. 提交前检查

文档改动至少执行：

```powershell
rg --files -g "*.md"
git diff --check
rg -n "check-fix-debug-list|00_InputFlow|InputFlow-项目规划|Steps\.md" README.md docs
```

预期结果：除根 `README.md` 外，所有 Markdown 都在 `docs/`；活动文档没有旧路径；
`git diff --check` 无错误。若 archive 正文保留历史路径，必须由 archive 提示说明其上下文。
