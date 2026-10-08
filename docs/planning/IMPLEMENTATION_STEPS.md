# InputFlow 当前执行顺序

> 文档类型：稳定执行顺序
>
> 当前进度：只看 [`../status/CURRENT_STATUS.md`](../status/CURRENT_STATUS.md)

本文件不再保存 M0～M7 的历史勾选状态。旧的全阶段清单已经归档到
[`../archive/plans/LEGACY_IMPLEMENTATION_STEPS.md`](../archive/plans/LEGACY_IMPLEMENTATION_STEPS.md)。

## 执行纪律

- 每次只执行当前状态文件指定的一个任务入口。
- 自动测试、故障注入、脚本输入、真实物理输入和用户观察分开记录。
- Windows 特有行为必须提供真实目标程序复现步骤；单元测试不能冒充实机证据。
- Hook 热路径不得执行 GUI、磁盘、网络、无界分配、无界队列或等待 UI。
- 配置保持向后兼容或提供显式迁移；不自动提升权限、不安装驱动、不修改系统输入设置。
- 每个阶段完成后归档任务卡和记录，再更新当前状态；不要在多个规划文件同步复制状态。

## 剩余顺序

1. **Phase F F4／F5**：按
   [`../tasks/PHASE_F_ACCEPTANCE.md`](../tasks/PHASE_F_ACCEPTANCE.md) 完成物理方向与短时资源验收。
2. **G-PRE**：按 [`../tasks/FIRST_RELEASE.md`](../tasks/FIRST_RELEASE.md) 完成有限自动回归、混合输入和恢复边界。
3. **H**：固定 x64 分发模式，完成依赖、路径、自启动、升级、移除和干净环境验收。
4. **RC**：固定提交和最终包，完成发布 smoke、版本、许可、说明和 SHA-256。
5. **首个 release**：建议 `v0.9.0-beta.1` Pre-release；实际远端动作仍需要对应授权。
6. **G-POST**：首版实际发布后按
   [`../tasks/POST_RELEASE_SOAK.md`](../tasks/POST_RELEASE_SOAK.md) 执行 24／72 小时长测和加固。

24／72 小时长测不是 Phase F、G-PRE、H、RC 或首个 release 的前置条件。发布前发现的真实丢键、
粘键、死锁、紧急旁路失效或配置损坏仍然阻断首版。
