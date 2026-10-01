# InputFlow 文档交接与 Codex 执行入口

> 日期：2026-10-02（Australia/Brisbane）  
> 适用基线：用户提供的 `input-flow(3).zip`；实际 Git HEAD、分支及未提交改动以 Codex 打开的仓库为准。  
> 本次交付只有 Markdown 文档，不包含代码变更、测试执行结果或重新打包的项目。

## 1. 用户已经确定的顺序

1. **首个 release 必须包含鼠标方向规则。**
2. Phase F 完成后执行发布前的短时回归、必要异常恢复和交付验证。
3. **24／72 小时长时间运行测试移到首个 release 发布之后，不作为首版发布或 RC 的前置条件。**
4. 首版定位为公开测试版本，默认建议 `v0.9.0-beta.1`，GitHub 标记为 Pre-release；这只是版本规划，不代表已经发布或承诺长期稳定。
5. WinUI 3 设置程序已经完成 Phase E；继续扩展现有程序，不重新开发一套 UI。

不得把发布后的长测重新塞进发布前清单，也不得为了赶发布省略鼠标方向、核心输入正确性或最终分发包验收。发布前若发现真实丢键、粘键、死锁、配置损坏等阻断缺陷，必须修复。

## 2. 将本次文件放回仓库

本表的路径均相对于仓库根。先核对本地工作区，保留用户的新改动；如果本地文档比本次基线更新，合并对应规划修改，不整份覆盖更新内容。

| 操作 | 仓库目标路径 | 用途 |
|---|---|---|
| 新增 | `00_InputFlow-文档交接与执行入口.md` | 本文件：拷贝说明、阅读顺序、完整性核对 |
| 新增 | `docs/RELEASE_ROADMAP.md` | 当前发布路线与阶段门槛 |
| 新增 | `check-fix-debug-list/tag_6_InputFlow-Phase-F-鼠标方向实施任务.md` | 下一步主任务：设计、实现、UI 和实机验收 |
| 新增 | `check-fix-debug-list/tag_6_InputFlow-首个Release收尾与发布任务.md` | G-PRE、H、RC 与首版发布准备 |
| 新增 | `check-fix-debug-list/tag_6_InputFlow-发布后Phase-G长时间运行任务.md` | 首版发布后的长测及修复 |
| 新增 | `check-fix-debug-list/tag_6_InputFlow-Phase-F与首版发布执行记录.md` | 待填写记录模板，不能当作已执行证据 |
| 修改 | `README.md` | 重定向当前 Codex 任务并说明首版范围 |
| 修改 | `Steps.md` | 追加剩余步骤，替换旧 MVP 发布门槛 |
| 修改 | `InputFlow-项目规划.md` | 同步用户最新发布决定 |
| 修改 | `docs/PROJECT_PLAN.md` | 与根目录项目规划保持逐字同步 |
| 修改 | `docs/BUILD_WINDOWS.md` | 区分开发构建和待验收的最终分发 |
| 修改 | `check-fix-debug-list/tag_5_InputFlow-M7-WinUI3架构与输入扩展任务.md` | 明确 A–E 是历史任务，F 和收尾转到 tag_6 |

不修改 M6、Phase E 的历史验收记录，不把尚未开始的 Phase F、发布验收或长测勾为完成。原 `tag_5.1` 文件保留供查阅；新的入口不再把它当作当前实施任务。

## 3. Codex 第一次阅读顺序

1. 本文件和 `docs/RELEASE_ROADMAP.md`。
2. 仓库实际适用的 `AGENTS.md`、Git 状态和最新代码。
3. `README.md`、两份项目规划、`Steps.md`、`docs/BUILD_WINDOWS.md`。
4. `check-fix-debug-list/M6-可靠性基线摘要.md`、`M7-Phase-E与M6-Windows实机验收记录.md`、`tag_5_InputFlow-M7-WinUI3架构与输入扩展记录.md`。
5. ADR-000～007；当前 engine/config/runtime/windows/protocol/agent/WinUI 代码。
6. `check-fix-debug-list/tag_6_InputFlow-Phase-F-鼠标方向实施任务.md`，从 F0 开始。

当前任务按 F → G-PRE → H → RC → 首个 Pre-release → G-POST 推进。无需等待 24／72 小时长测，也不在 F 中夹带 URL、启动程序或其他新功能。

## 4. 可直接交给 Codex 的入口提示词

> 请先阅读仓库根的 `00_InputFlow-文档交接与执行入口.md` 与 `docs/RELEASE_ROADMAP.md`，核对实际 Git 状态和适用的 `AGENTS.md`，再执行 `check-fix-debug-list/tag_6_InputFlow-Phase-F-鼠标方向实施任务.md`。用户已确定首个 release 包含鼠标方向，24／72 小时长测在首个 release 发布之后。保留现有 M6 与 Phase E 正确性修复；按设计、测试、实现、实机验证的顺序推进。每个阶段更新 `tag_6_InputFlow-Phase-F与首版发布执行记录.md`，明确已执行、继承基线、未执行和环境限制。F 完成后进入首个 Release 收尾任务；先完成可审阅的产物和发布草稿。实际远端发布、消息发送和标签推送依据用户对这些动作的授权，不把本文的开发提示词视为已经获得远端发布授权。

## 5. 完整性与权限边界

- 本次共 12 份文件；两份项目规划应内容一致。
- 本次仅调整文件中的后续规划；报告中已有 Rust 151／151 和 C# 17／17 是历史记录，不是本次重新运行的测试。
- 新任务书要求的脚本、ADR、Schema、安装或自启动能力均是待实现内容；不要在 README 写成已经支持。
- 文档和 ADR 使用中文；代码标识符、注释、测试名及提交信息使用英文。
- 自动测试、故障注入、脚本输入、真实物理输入和用户观察分开记录。
- 正常的本地开发、文档更新和隔离测试可以持续推进；系统级设置、签名身份、许可证选择及远端发布等实际需要用户决定的事项，先完成独立工作和可审阅结果，不反复询问已确认的产品方向。
