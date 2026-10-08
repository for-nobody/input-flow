# InputFlow 当前状态

> 文档类型：当前进度唯一权威
>
> 最后更新：2026-10-08（Australia/Brisbane）
>
> 实现基线：`main`，提交 `3c3a158821614419ec6b5b4b000f94c8d9f6cbdf`

## 当前结论

InputFlow 已完成 M1～M7。Phase F 的设计、代码、Schema v4、IPC 能力声明、WinUI
编辑／有限预览和自动验证已经完成；Phase F 尚未完成，因为 F-PHY-01～07 的真实 Windows
输入、普通拖拽和约五分钟物理鼠标移动观察仍未执行。项目尚未进入 G-PRE、分发阶段 H 或
RC，也尚未发布首个 release。

当前唯一执行入口是
[`../tasks/PHASE_F_ACCEPTANCE.md`](../tasks/PHASE_F_ACCEPTANCE.md)。

## 阶段状态

| 阶段 | 状态 | 完成或进入条件 |
|---|---|---|
| M1～M6 | 完成 | 原型、匹配器与可靠性加固已收口；证据已归档 |
| M7 Phase A～E | 完成 | Rust/Win32 agent、Named Pipe、完整键盘、WinUI 设置和联合验收已收口 |
| Phase F F0～F3 | 完成 | ADR-008、方向 matcher／Hook、Schema v4、IPC、WinUI 和自动 contract 已落地 |
| Phase F F4／F5 | **进行中** | F-PHY-01～07 和受影响现场回归通过并写入 Phase F 记录 |
| G-PRE | 未开始 | Phase F 完成后执行短时可靠性与恢复验收 |
| H | 未开始 | 完成分发、依赖、路径、自启动、升级／移除和干净环境验证 |
| RC／首个 release | 未开始／未发布 | 固定最终包、完成 smoke、说明、许可和校验和后按授权发布 |
| G-POST | 计划于首版发布后 | 24／72 小时长测和 daily-drive，不阻塞首版 |

## 最近已验证证据

提交 `3c3a158` 对应的 Phase F 自动结果：

- Rust workspace：169／169；`fmt`、Clippy（warnings denied）和 Release agent 构建通过。
- C# protocol contract：6／6；Settings Core：11／11。
- WinUI solution Debug／Release：0 warning、0 error。
- `scripts/build-windows.ps1 -SkipRestore`：通过。
- 合成 125／500／1000 Hz 序列只证明固定空间和确定性行为，不是物理 polling rate 证据。

完整命令、实现说明和限制见 [`../records/PHASE_F.md`](../records/PHASE_F.md)。M6/M7 的
历史证据见 [`../archive/m6/`](../archive/m6/) 和 [`../archive/m7/`](../archive/m7/)。

## 当前未验证边界

- 真实鼠标四方向、距离不足、偏轴、超时和一次按住只命中一次。
- 候选期间 F12、pause、配置替换、预览取消和正常退出。
- 普通点击、菜单、拖拽、多屏／跨 DPI 和约五分钟混合物理 move。
- keypad Enter、独立播放键、中文 Narrator 语音环境、partial `SendInput` 等既有硬件／环境限制。
- 最终分发包、干净环境、自启动、升级、移除、签名／许可和远端 release。
- 24／72 小时长测；它明确安排在首版发布后。

## 下一步

执行 F-PHY-01～07，把环境、目标应用、真实输入序列、资源口径和结果写入 Phase F 记录。
全部通过后归档 Phase F 任务和记录，更新本文为“Phase F 完成”，再进入
[`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md) 的 G-PRE。
