# InputFlow

Windows 全局键盘与鼠标输入组合引擎：观察键鼠事件，暂扣可能构成已启用规则的事件，命中后消费并发送动作，失败/超时则按序回放。

> 完整项目上下文见 `docs/PROJECT_PLAN.md`；可执行开发计划见 `Steps.md`。

## 当前状态

- 里程碑：Step 4（M4）—— 组合匹配。
- 下一步：Step 5（M5）—— 时序规则。

## 开发环境（首次实现时固定，满足 NFR-06）

| 项目 | 实测值 |
|---|---|
| 操作系统 | Microsoft Windows 11 Pro，10.0.26200（build 26200），64-bit |
| CPU 架构 | AMD64（x86_64） |
| Rust | rustc 1.98.1 (48a229cea 2026-09-01)、cargo 1.98.1 (797e8a9bc 2026-08-05) |
| Rust 工具链 | `stable-x86_64-pc-windows-msvc`（active/default），target `x86_64-pc-windows-msvc` |
| MSVC/C++ 构建工具 | 未单独安装（无 `cl.exe`/`link.exe`）；构建经 Rust 1.98.1 内置 `rust-lld` + `windows-link` 自包含链接完成并验证通过 |
| Windows SDK | 10.0.26100.0 |
| git | 2.55.0.windows.5 |

## 锁定依赖（写入 Cargo.lock）

| crate | 版本 | 说明 |
|---|---|---|
| `windows-sys` | 0.61.2 | windows-rs 的原始 FFI 绑定（M1 启用 `Win32_Foundation`、`Win32_System_LibraryLoader`、`Win32_System_Threading`、`Win32_UI_Input_KeyboardAndMouse`、`Win32_UI_WindowsAndMessaging`） |
| `windows-link` | 0.2.1 | 传递依赖，负责解析导入库链接 |

## 构建与运行

```powershell
# 全量测试（纯引擎单测 + probe-cli 回归单测）
cargo test --workspace

# 构建 / 运行 probe-cli
cargo build -p probe-cli
cargo run -p probe-cli
```

预期输出形如：

```text
probe-cli: low-level hooks installed on message-loop thread 6632.
probe-cli: hold LeftCtrl and click the right mouse button to send Ctrl+C.
probe-cli: non-matching chords (e.g. Ctrl+Q) are replayed in order on failure.
probe-cli: F12 toggles bypass; type `quit` and press Enter to exit.
probe-cli: shut down cleanly. observed 0 events; 0 output batch(es) sent, 0 failed, 0 dropped.
```

在控制台输入 `quit`（或 `exit`/`q`）并回车即干净退出（退出码 0）。M4 演示规则为 `LeftCtrl + RightButton → Ctrl+C`：按住左 Ctrl 时按下鼠标右键，命中后消费触发输入并经 `SendInput` 发送 `Ctrl+C`（无原右键菜单）；不匹配的组合（如 Ctrl+Q）按序回放 `[Ctrl, Q]`，无候选键直接放行。注入事件带 `dwExtraInfo=0x494E5055` 标记，不递归；F12 切换旁路，`SendInput` 失败也会进入旁路。

## 仓库结构

```text
inputflow/
├── README.md
├── Steps.md
├── InputFlow-项目规划.md
├── .gitignore
├── Cargo.toml               # Cargo workspace（resolver = "3"）
├── Cargo.lock
├── docs/
│   ├── PROJECT_PLAN.md       # 项目上下文（自规划稿复制）
│   ├── glossary.md           # 术语表
│   ├── research-log.md       # 研究日志
│   └── decisions/
│       ├── adr-template.md
│       └── ADR-000-仓库结构与技术选型.md
├── crates/
│   ├── inputflow-engine/     # 纯逻辑引擎（无 Windows 依赖，跨平台单测）
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── event.rs      # 平台无关 InputEvent / Key / MouseKind / MouseButton
│   │       ├── pending.rs    # 有界暂扣队列（FIFO、溢出旁路）
│   │       ├── state.rs      # 按键/鼠标按键：物理按住 / 已见 / 已消费
│   │       ├── rules.rs      # Trigger/Rule/RuleError/RuleIndex 预编译 + 冲突检测
│   │       └── matcher.rs    # 纯状态机：Decision/Resolution、可注入时钟
│   └── inputflow-windows/    # M4 起承载平台接入（全部 unsafe 集中于此）
│       └── src/
│           ├── lib.rs
│           ├── keymap.rs             # VK↔Key、鼠标消息映射（纯逻辑，可单测）
│           └── platform/
│               ├── mod.rs
│               └── windows.rs        # Hook 安装/卸载、消息循环、回调、SendInput
└── apps/
    └── probe-cli/            # M1-M5 原型（workspace 成员）
        ├── Cargo.toml
        └── src/
            └── main.rs               # 线程编排、规则定义、logger、输出 worker、退出
```

## 已知限制 / 备注

- 当前无需安装完整 MSVC C++ 构建工具即可构建（Rust 自包含链接）；若后续里程碑（如 Tauri 2 或原生依赖）需要完整 MSVC 工具链，再安装 VS 2022 Build Tools 的「使用 C++ 的桌面开发」工作负载并回写版本号。
- `SendInput` 受 UIPI 完整性级别限制：聚焦提升权限（管理员）窗口时注入可能被拒绝或忽略；失败时程序进入旁路状态并记录。
- 语言约定：文档 / ADR / 研究日志用中文；代码注释、标识符、提交信息、测试名用英文。
- M4 组合语义（ADR-001）：首键暂扣、第二输入完成则消费并发送动作、否则按序回放 `[首键, 第二输入]`；组合无显式超时、不支持重叠前缀，这些留给 M5/M6。`Key` 仅覆盖常用键，未建模键以 `Unknown(vk)` 兜底。
- 自 M3 起仓库为 Cargo workspace：`inputflow-engine`（纯逻辑、零依赖、跨平台单测）、`inputflow-windows`（M4 起承载全部 `unsafe` Win32）、`probe-cli`（原型）。引擎单测（组合命中/失败/多候选/冲突/溢出/暂停等）+ keymap 单测共 34 项通过。
