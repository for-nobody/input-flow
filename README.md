# InputFlow

Windows 全局键盘与鼠标输入组合引擎：观察键鼠事件，暂扣可能构成已启用规则的事件，命中后消费并发送动作，失败/超时则按序回放。

> 完整项目上下文见 `docs/PROJECT_PLAN.md`；可执行开发计划见 `Steps.md`。

## 当前状态

- 里程碑：Step 6（M6）—— 可靠性。
- 下一步：Step 7（M7）—— 桌面界面。

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
| `serde` | 1.0 | 配置序列化 / 反序列化（derive） |
| `serde_json` | 1.0 | JSON 配置解析与生成 |

## 构建与运行

```powershell
# 全量测试（引擎 + 配置 + keymap + probe-cli）
cargo test --workspace

# 构建 / 运行 probe-cli
cargo build -p probe-cli
cargo run -p probe-cli -- --config %LOCALAPPDATA%\InputFlow\config.json
```

- 配置默认从 `%LOCALAPPDATA%\InputFlow\config.json` 读取；`--config PATH` 覆盖，`--print-default-config` 打印模板，`--debug` 打开逐键调试日志。
- 控制台命令：`pause`（暂停并冲刷已暂扣输入）、`resume`（恢复）、`stats`（打印 p50/p95/p99）、`quit`/`exit`/`q`（干净退出，退出码 0）。
- 缺失或损坏的配置不会阻止启动，会回退为「空规则旁路」并打印警告。

预期输出形如：

```text
probe-cli: low-level hooks installed on message-loop thread 5116.
probe-cli: 1 rule(s) loaded from `...\config.json`.
probe-cli: emergency bypass key `F12`.
probe-cli: type `pause`, `resume`, `stats`, or `quit`.
probe-cli: shut down cleanly. observed 0 events; 0 output batch(es) sent, 0 failed, 0 dropped.
probe-cli: callback latency (us): total=0 p50=- p95=- p99=-
probe-cli: hold delay (us): total=0 p50=- p95=- p99=-
```

M6 演示规则仍为 `Hold(LeftCtrl, 250ms) + RightButton → Ctrl+C`（见下方配置示例）：按住左 Ctrl ≥250ms 后点右键命中，消费触发输入并经 `SendInput` 发送 `Ctrl+C`（无原右键菜单）；不匹配的组合（如 Ctrl+Q）按序回放 `[Ctrl, Q]`，无候选键直接放行。注入事件带 `dwExtraInfo=0x494E5055` 标记，不递归；紧急键（默认 F12）与 `pause` 都会暂停并冲刷已暂扣输入，`SendInput` 失败也会进入旁路。`Hold`/`Hold+Button` 的到期由消息循环线程的 `SetTimer`（约 5ms）驱动。

配置示例（`config.json`）：

```json
{
  "schema_version": 1,
  "emergency_bypass_key": "F12",
  "rules": [
    {
      "id": "hold-ctrl-right-click-copy",
      "trigger": { "type": "hold_mouse_button", "key": "LeftCtrl", "timeout_ms": 250, "button": "Right" },
      "action": { "type": "key_chord", "keys": ["LeftCtrl", "C"] }
    }
  ]
}
```

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
│       ├── ADR-000-仓库结构与技术选型.md
│       ├── ADR-001-组合匹配与回放协议.md
│       └── ADR-002-时序规则与冲突策略.md
├── crates/
│   ├── inputflow-engine/     # 纯逻辑引擎（无 Windows 依赖，跨平台单测）
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── event.rs      # 平台无关 InputEvent / Key / MouseKind / MouseButton + 键名映射
│   │       ├── pending.rs    # 有界暂扣队列（FIFO、溢出旁路）
│   │       ├── state.rs      # 按键/鼠标按键：物理按住 / 已见 / 已消费
│   │       ├── rules.rs      # Trigger/Rule/RuleError/RuleIndex 预编译 + 冲突检测
│   │       ├── matcher.rs    # 纯状态机：Decision/Resolution、可注入时钟
│   │       └── stats.rs      # PercentileTracker：有界样本 + p50/p95/p99
│   ├── inputflow-config/     # M6 起：版本化 JSON 配置、校验、原子保存、坏文件回退
│   │   └── src/
│   │       ├── lib.rs
│   │       └── config.rs     # Config/RuleConfig/TriggerConfig/ActionConfig + load/save/validate
│   └── inputflow-windows/    # M4 起承载平台接入（全部 unsafe 集中于此）
│       └── src/
│           ├── lib.rs
│           ├── keymap.rs             # VK↔Key、鼠标消息映射（纯逻辑，可单测）
│           └── platform/
│               ├── mod.rs
│               └── windows.rs        # Hook 安装/卸载、消息循环、回调、SendInput、暂停/旁路、性能采样
└── apps/
    └── probe-cli/            # M1-M6 原型（workspace 成员）
        ├── Cargo.toml
        └── src/
            └── main.rs               # 线程编排、配置加载、logger、输出 worker、控制台命令、退出
```

## 已知限制 / 备注

- 当前无需安装完整 MSVC C++ 构建工具即可构建（Rust 自包含链接）；若后续里程碑（如 Tauri 2 或原生依赖）需要完整 MSVC 工具链，再安装 VS 2022 Build Tools 的「使用 C++ 的桌面开发」工作负载并回写版本号。
- `SendInput` 受 UIPI 完整性级别限制：聚焦提升权限（管理员）窗口时注入可能被拒绝或忽略；失败时程序进入旁路状态并记录。
- 语言约定：文档 / ADR / 研究日志用中文；代码注释、标识符、提交信息、测试名用英文。
- 时序规则（ADR-002）：`Hold(K,T)` 重复 down 不重置计时、`T` 前释放不命中；`Hold(K,T)+Button(B)` 需 `T` 后且 `K` 仍按住再 `B down` 才命中，MVP 不允许“先按 B 再达到 T”。一个前缀键最多属于一种规则种类：单键 `Hold{K}` 与同前缀复合规则（含 `HoldMouseButton`）、`HoldMouseButton{K}` 与同前缀和弦均判为冲突并拒绝启用。
- `Hold` / `Hold+Button` 的到期由消息循环线程上的周期 `SetTimer`（约 5ms）驱动；精确一次性调度仍留待后续，待 M6 性能基线数据决定是否值得。
- 组合无显式超时、不支持重叠前缀；`Key` 仅覆盖常用键，未建模键以 `Unknown(vk)` 兜底。
- 可靠性（ADR-003）：缺失/损坏配置回退「空规则旁路」并可启动；紧急键（默认 F12，可配置）与 `pause` 都会先冲刷已暂扣输入再停止拦截；诊断日志默认匿名化，逐键细节仅在 `--debug` 下输出；崩溃标记在下次启动提示上次异常终止；`stats` 报告回调耗时与暂扣延迟的 p50/p95/p99 基线。
- 自 M3 起仓库为 Cargo workspace：`inputflow-engine`（纯逻辑、零依赖、跨平台单测）、`inputflow-config`（版本化 JSON 配置、校验、原子保存、坏文件回退）、`inputflow-windows`（承载全部 `unsafe` Win32）、`probe-cli`（原型）。单测：引擎 48 项 + 配置 9 项 + keymap 3 项共 60 项通过。
