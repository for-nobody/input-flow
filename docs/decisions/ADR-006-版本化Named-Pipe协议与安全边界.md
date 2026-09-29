# ADR-006：版本化 Named Pipe 协议与安全边界

- 状态：已接受（Accepted）
- 日期：2026-09-30
- 涉及模块：`inputflow-protocol`、`inputflow-windows`、`inputflow-agent`、`InputFlow.Protocol`

## 背景（Context）

Phase C 已经让 Rust agent 成为唯一 Hook、matcher、正式配置、托盘和诊断所有者，并把 pause、resume、规则替换和 capture 串行到 Hook owner。Phase D 需要让按需启动的 WinUI 设置程序访问这些能力，但不能让 IPC 线程进入 Hook callback、直接写配置文件、执行前端给出的路径/命令，或在断线时改变现有规则。

协议必须在以下失败中仍有明确边界：流式传输拆包/粘包、超长或损坏 JSON、旧客户端、重复请求、多个客户端竞争、客户端超时/取消/崩溃、capture 所有者断线，以及 agent 关闭。默认 Named Pipe 安全描述符会向 Everyone/匿名主体授予读权限，因此不能使用默认 ACL。

## 候选方案（Options）

### 方案 A：JSON Lines（每行一个 JSON）

- 人工调试简单，普通文本工具可直接读写。
- JSON 字符串本身可包含转义换行，但实现仍须正确区分转义内容与帧分隔；读取方必须一直扫描到分隔符才能知道消息大小。
- 恶意客户端可在不发送换行的情况下持续占用缓冲区；虽然可以人为设置扫描上限，错误后的重新同步和部分帧处理更复杂。

### 方案 B：4 字节长度前缀 + UTF-8 JSON

- 固定头部先给出精确长度，可在分配 payload 前拒绝 0 长度和超长消息；字节模式 Named Pipe 的拆包/粘包不影响消息边界。
- 部分头、部分 payload 和断线都能确定地区分；同一 codec 可在 Rust 与 C# 中实现。
- 人工查看不如 JSON Lines 直接，必须先处理 4 字节头；长度端序和上限必须成为协议的一部分。

### 方案 C：Message-mode Named Pipe + 每次写一个 JSON

- Windows 能保留每次写入的消息边界，表面实现较短。
- 客户端必须正确切换 message read mode 并处理 `ERROR_MORE_DATA`；不同语言库的默认模式不同，C# 客户端和测试工具更容易产生隐含差异。
- 协议边界依赖传输模式，未来若需要内存流/测试流或替代本地传输，codec 不可直接复用；消息大小仍需自行限制。

### 选择

采用方案 B：**byte-mode Named Pipe 上的 4 字节 little-endian 无符号长度 + UTF-8 JSON**。它在输入正确性、固定内存上界、跨语言一致性和可测试性之间最清楚。最大 JSON payload 为 1 MiB；长度不包含 4 字节头。超长帧不分配、不尝试排空，返回 `message_too_large` 后关闭连接。

## 最终决策（Decision）

### 1. 连接与命名

- 协议版本为 `1`，pipe 名为 `InputFlow.Agent.v1.<session-id>`；完整 Win32 路径是 `\\.\pipe\InputFlow.Agent.v1.<session-id>`。session id 避免同一账户的多个交互会话互相连接。
- server 使用 byte type/read mode、overlapped I/O、最多 8 个并发实例和 `PIPE_REJECT_REMOTE_CLIENTS`。
- 每个连接必须先发送 `handshake`。handshake 前的其他方法返回 `handshake_required`；重复 handshake 返回 `already_handshaken`。
- 每个请求 envelope 必须有 `protocol_version`、非空 `request_id`、`method` 和对象形态的 `params`。request id 是最多 128 个字符的字符串，避免跨语言 JSON 整数精度问题。
- response 回显 request id；错误包含稳定的 `code`、可展示的 `message`，可选 `details`。无法解析 request id 的损坏帧使用空 request id。

### 2. 版本、重复请求与顺序

- v1 只接受精确版本 `1`。版本不匹配返回 `unsupported_protocol` 并关闭连接；handshake 响应列出 server/protocol/schema 版本、能力和大小/队列限制。
- request id 在**单连接内**必须唯一。server 为每个连接最多保存 4096 个 id；重复 id 返回 `duplicate_request_id`，绝不重放缓存结果，也不再次执行 mutation。达到上限返回 `connection_request_limit` 并要求重连。
- 同一连接串行处理请求。不同连接可并发解析；所有 runtime mutation 仍由现有 apply lock 与 Hook-owner control queue 决定实际顺序。协议不建立一个绕过 runtime 的第二状态源。
- v1 不自动重试任何 mutation。客户端在写入后超时或断线时，结果可能已经提交；它必须重连并用 `get_status` / `get_config` 核对，再由用户发起新的 request id。

### 3. 方法与事务

v1 方法集：

- `handshake`
- `get_status`
- `get_config`
- `validate_config`
- `apply_config`
- `pause`
- `resume`
- `get_stats`
- `begin_capture`
- `cancel_capture`
- `subscribe_events`

`validate_config` 只调用 Rust 权威校验，不落盘、不替换规则。`apply_config` 复用 Phase C 的事务：权威 validate/compile → 原子 temp/commit → Hook owner 冲刷 pending 并替换 → 返回结构化 save/runtime/rollback 报告。已开始但 acknowledgement 超时的替换继续由 runtime reconciliation 收敛；IPC 超时不能把它错误回滚或重放。

请求不包含文件路径、shell 命令、任意资源名或通用文件访问。`get_config` 只返回 agent 当前内存中的正式 v2 配置；UI 只能把完整草稿交给 validate/apply。

### 4. 超时、取消、断线与关闭

- Rust server 不为已经进入 runtime 的 mutation 伪造取消。C# client 默认 response deadline 为 3 秒；deadline 或调用方取消会关闭该 client connection，防止迟到 response 污染下一次请求，但只表示客户端停止等待，不证明服务端没有执行。
- v1 的显式取消只用于 capture：`begin_capture.timeout_ms` 范围为 100–30000 ms，`cancel_capture` 必须给出 session id。runtime 仍保证全 agent 同时最多一个 capture。
- capture 归属于发起它的 connection。连接 EOF/损坏关闭时 server 尽快调用 runtime cancel；runtime 自身的 deadline 是最终保障。capture 只观察一个非注入、非紧急键的 down，不暂停 matcher、不吞输入。
- 普通 UI 连接断开不 pause、不恢复、不替换配置，也不终止 agent。重新连接必须重新 handshake，使用新的 request id；当前规则保持不变。
- agent 关闭先进入 closing 状态，拒绝新 dispatch 为 `agent_shutting_down`，推送 `server_shutting_down`，短暂留出响应窗口后唤醒 overlapped accept/read/write 并关闭连接。若连接只观察到 EOF，读操作视为 server 已关闭；未收到 response 的 mutation 仍按“结果未知，重连核对”处理。

### 5. 有界事件推送

- `subscribe_events` 成功后该连接转换为只读 event stream，不再接收普通 request；需要控制请求时使用另一条已 handshake 的连接。
- 每个订阅有 32 项有界队列，全 agent 最多 8 个订阅。发布使用 `try_send`，绝不等待订阅者；慢客户端只丢推送，不影响 Hook/runtime。
- event 有单调 `event_id`。客户端用 id 跳变识别丢失并主动 `get_status`/`get_config` 重同步。事件至少包含 status change、capture terminal result、config applied 和 server shutting down。
- stream 每秒发送 heartbeat，使已经断开的慢/空闲订阅能在有界时间内被发现；heartbeat 不携带输入 identity。
- F12、托盘和 IPC 控制造成的暂停状态都由同一个 runtime `state_revision` 观察并合并发布，不维护 IPC 专用暂停布尔值。

### 6. ACL 与资源安全

- 每个 pipe instance 都带显式 protected DACL：当前 agent 进程 token 的用户 SID和 LocalSystem 具有访问权；不使用默认 security descriptor，也不授予 Everyone/Anonymous。
- 当前实现再用 session-qualified pipe name 和 `PIPE_REJECT_REMOTE_CLIENTS` 缩小范围。ACL 构造、SID 读取、security descriptor、overlapped event、pipe handle 和取消/关闭都集中在 `inputflow-windows::platform::pipe`。
- 首个 instance 使用 `FILE_FLAG_FIRST_PIPE_INSTANCE`，降低同 session 同用户抢占 server 名称的风险；agent 的既有 Local named mutex 仍负责产品单实例。
- Hook callback 不调用 protocol、pipe 或 event hub。IPC worker 只解析、校验和调用 runtime 的公开控制面；所有队列、连接数、frame 和 request-id 集均有上限。

## 协议形状（Protocol Shape）

请求示例：

```json
{
  "protocol_version": 1,
  "request_id": "settings-42",
  "method": "get_status",
  "params": {}
}
```

成功响应示例：

```json
{
  "protocol_version": 1,
  "request_id": "settings-42",
  "type": "success",
  "result": {}
}
```

错误示例：

```json
{
  "protocol_version": 1,
  "request_id": "settings-42",
  "type": "error",
  "error": {
    "code": "handshake_required",
    "message": "handshake must be the first request on a connection"
  }
}
```

Rust serde DTO/codec 是 wire contract 的权威实现；`fixtures/protocol/v1/` 是 Rust/C# 共同读取的 golden contract。C# client 只负责 framing、DTO 和连接生命周期，不复制 Rust 配置校验。

## 失败模型与测试（Failure Model and Tests）

- 部分 header/payload：codec 循环读取；中途 EOF 返回 `truncated_frame` 并关闭。
- 0 长度/坏 UTF-8/坏 JSON/未知字段或 params：返回机器错误；边界仍可确定时允许下一帧。
- 超长：不分配 payload，错误后关闭。
- 版本错误：明确拒绝并关闭，不尝试猜测兼容。
- 并发：独立 connection worker；慢订阅、断开的 UI 或错误 JSON 不占用 Hook owner。
- 重复 mutation：同连接 duplicate id 在 dispatch 前拒绝；跨重连不自动重试，客户端按结果未知处理。
- capture 断线：connection cleanup 发出 cancel；runtime timeout/shutdown 仍提供终态。
- shutdown：先 closing、后唤醒 overlapped I/O；worker 有界退出。

自动测试覆盖 codec round-trip/部分帧/超长/错误 JSON/严格 envelope/版本、实际 Windows Named Pipe 并发客户端、超时放弃、断线清理、重连、duplicate id 和 bounded subscription。C# contract runner 读取同一 golden fixtures 并验证 framing 和 config JSON 的无语义损失往返。

## 后果（Consequences）

### 正面

- 消息边界、内存上限、跨语言形状和错误均可确定测试。
- UI 崩溃、超时和重连不会隐式修改规则；mutation 的不确定结果不会被错误重放。
- 显式当前用户 ACL、session 名和 remote reject 避免默认 Named Pipe ACL 的过宽访问。
- event push 有界且与 Hook callback 完全隔离。

### 成本与限制

- Rust 与 C# 各维护一个很小的 length-prefix codec，必须持续通过 golden fixtures 防漂移。
- v1 不提供通用 request cancellation，也不在单连接上复用 request/stream；这牺牲少量便利换取明确顺序。
- 当前用户 SID 允许同一 Windows 用户会话内的其他进程连接；Named Pipe 不是对同用户恶意进程的安全隔离。协议仍严格限制可执行操作和输入大小。
- Phase D 只建立协议/client contract；正式 WinUI 页面、动态键名和规则编辑属于 Phase E。

## 官方依据

- Named Pipe security and access rights：https://learn.microsoft.com/windows/win32/ipc/named-pipe-security-and-access-rights
- Named Pipe type/read/wait modes：https://learn.microsoft.com/windows/win32/ipc/named-pipe-type-read-and-wait-modes
- CreateNamedPipeW：https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-createnamedpipew
- Overlapped input/output：https://learn.microsoft.com/windows/win32/sync/synchronization-and-overlapped-input-and-output
- Security Descriptor String Format：https://learn.microsoft.com/windows/win32/secauthz/security-descriptor-string-format
