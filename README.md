# InputFlow

Windows 全局键盘与鼠标输入组合引擎：观察键鼠事件，暂扣可能构成已启用规则的事件，命中后消费并发送动作，失败/超时则按序回放。

> 完整项目上下文见 `docs/PROJECT_PLAN.md`；可执行开发计划见 `Steps.md`。

## 当前状态

- 里程碑：Step 2（M2）—— 抑制与回放探针（代码完成，交互实机验证待做）。
- 下一步：Step 3（M3）—— 核心状态模型。

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
cd apps\probe-cli
cargo build
cargo run
```

预期输出形如：

```text
probe-cli: low-level hooks installed on message-loop thread 12345.
probe-cli: F8 is held ~300ms then replayed once; all other keys pass through.
probe-cli: F12 toggles bypass (stop intercepting); type `quit` and press Enter to exit.
[seq=000000] t=8461500ms kbd Down F8 vk=0x77 scan=0x42 ext=false repeat=false injected=false extra=0x0
probe-cli: replayed F8 (seq=000000); SendInput inserted 2/2 events.
[seq=000001] t=8461600ms kbd Down F8 vk=0x77 scan=0x42 ext=false repeat=false injected=true extra=0x494E5055
[seq=000002] t=8461600ms kbd Up F8 vk=0x77 scan=0x42 ext=false repeat=false injected=true extra=0x494E5055
probe-cli: bypass ON (interception stopped).
probe-cli: shut down cleanly. observed 3 events, dropped 0 (queue full); replayed 1 F8, 0 failed, 0 replay requests dropped.
```

在控制台输入 `quit`（或 `exit`/`q`）并回车即干净退出（退出码 0）。M2 只暂扣 F8：物理 F8 down/up 被抑制，约 300ms 后经 `SendInput` 回放一次 down+up 对；注入事件带 `dwExtraInfo=0x494E5055` 标记，不递归；F12 切换旁路，`SendInput` 失败也会进入旁路。

## 仓库结构

```text
inputflow/
├── README.md
├── Steps.md
├── InputFlow-项目规划.md
├── .gitignore
├── docs/
│   ├── PROJECT_PLAN.md       # 项目上下文（自规划稿复制）
│   ├── glossary.md           # 术语表
│   ├── research-log.md       # 研究日志
│   └── decisions/
│       ├── adr-template.md
│       └── ADR-000-仓库结构与技术选型.md
└── apps/
    └── probe-cli/            # M1-M5 原型（单包、非 workspace）
        ├── Cargo.toml
        ├── Cargo.lock
        └── src/
            ├── main.rs              # 线程编排、退出、logger、replay worker
            ├── event.rs             # 归一化 InputEvent + 按键名映射（无 unsafe）
            └── platform/
                ├── mod.rs
                └── windows.rs       # 全部 unsafe Win32：Hook 安装/卸载、消息循环、回调、SendInput 回放
```

## 已知限制 / 备注

- 当前无需安装完整 MSVC C++ 构建工具即可构建（Rust 自包含链接）；若后续里程碑（如 Tauri 2 或原生依赖）需要完整 MSVC 工具链，再安装 VS 2022 Build Tools 的「使用 C++ 的桌面开发」工作负载并回写版本号。
- M2 回放为固定延迟成对回放（约 300ms 后注入 F8 down+up），不保留实际按住时长；退出时未处理的暂扣回放请求会被丢弃。
- `SendInput` 受 UIPI 完整性级别限制：聚焦提升权限（管理员）窗口时注入可能被拒绝或忽略；失败时程序进入旁路状态并记录。
- 语言约定：文档 / ADR / 研究日志用中文；代码注释、标识符、提交信息、测试名用英文。
