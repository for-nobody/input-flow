# InputFlow IPC golden fixtures

`v1/` 是 ADR-006 的 Rust/C# 共同 wire contract。Rust codec 测试和
`InputFlow.Protocol.ContractTests` 必须读取这些文件；修改字段、tag、版本或配置形状时，
必须同时更新 ADR、Rust DTO、C# client 和 fixture，不能只修改某一语言的副本。

Frame 的 4 字节 little-endian 长度不写入 fixture；fixture 内容就是长度所覆盖的 UTF-8
JSON payload。`get-config-response.json` 内嵌内容必须与 `fixtures/config/v4-valid.json`
语义相等。
