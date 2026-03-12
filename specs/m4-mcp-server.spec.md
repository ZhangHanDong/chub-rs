spec: task
name: "m4-mcp-server"
inherits: chub-rs
---

## 意图

实现独立的 `chub-mcp` 二进制，通过 stdio JSON-RPC 暴露 Context Hub 的 search/get/list/
annotate/feedback 能力，让 Claude Code、Cursor 等 MCP 客户端可以直接调用与 CLI 一致的行为。

## 决策

- MCP server 作为单独的 bin target `chub-mcp` 发布。
- 所有日志、调试信息和意外输出都必须重定向到 stderr，避免污染 stdio JSON-RPC 协议。
- tool 名称保持 `chub_search`、`chub_get`、`chub_list`、`chub_annotate`、`chub_feedback`。
- resource 名称保持 `chub://registry`，返回去除内部字段后的简化摘要。
- tool 返回值优先复用 CLI/lib 逻辑，只在 MCP 层做参数校验、结果包装和协议转换。
- server 启动时做 best-effort registry 初始化；初始化失败时 server 仍然启动，但后续请求可返回错误结果。

## 边界

### 允许修改
- `Cargo.toml`
- `src/bin/chub_mcp.rs`
- `src/mcp/**`
- `tests/**`

### 禁止做
- 不要在 MCP 层重新实现一套与 CLI 不一致的业务逻辑。
- 不要让 stdout 上出现非 JSON-RPC 的日志文本。

## 验收标准

场景: MCP server 可以完成初始化握手
  测试: mcp_server_initialize
  假设 已编译 `chub-mcp` 二进制
  当 客户端发送 initialize 请求
  那么 响应包含 server name=`chub`、版本号和声明过的 capabilities

场景: `chub_search` 返回简化搜索结果
  测试: mcp_tool_search_returns_simplified_results
  假设 registry 已加载且存在可搜索条目
  当 调用 `chub_search`，传入 `query` 和 `limit`
  那么 返回 `{ results, total, showing }`，且每个结果仅包含 agent 需要的公开字段

场景: `chub_get` 返回文本内容并要求 doc 显式语言
  测试: mcp_tool_get_text_and_lang_errors
  假设 registry 中存在 doc 条目和 skill 条目
  当 调用 `chub_get` 获取 skill、获取带 `lang` 的 doc，以及获取缺少 `lang` 的 doc
  那么 skill 和合法 doc 返回文本内容，缺少 `lang` 的 doc 返回 `isError=true` 和可用语言列表

场景: `chub_get` 阻止路径穿越
  测试: mcp_tool_get_blocks_path_traversal
  假设 MCP server 已初始化
  当 调用 `chub_get` 并传入包含路径穿越片段的 `file`
  那么 返回错误结果，并明确指出不允许 path traversal

场景: `chub_list` 返回条目列表与过滤后的总数
  测试: mcp_tool_list_returns_entries
  假设 registry 中存在多条 doc/skill
  当 调用 `chub_list` 并传入 `lang` 或 `limit`
  那么 返回 `{ entries, total, showing }`，并正确应用过滤和限制

场景: `chub_annotate` 支持读写清列四种模式
  测试: mcp_tool_annotate_modes
  假设 `CHUB_DIR` 指向临时目录
  当 分别调用 list、read、write 和 clear 四种模式
  那么 返回的 `status`、`annotation` 和 `annotations` 字段与对应模式一致

场景: `chub_annotate` 校验 ID 长度与字符集
  测试: mcp_tool_annotate_validates_entry_id
  假设 MCP server 已初始化
  当 调用 `chub_annotate` 并传入 201 字符 ID 或包含非法字符的 ID
  那么 返回 `isError=true`，并说明过长或非法字符错误

场景: telemetry 禁用时 `chub_feedback` 返回 skipped
  测试: mcp_tool_feedback_skips_when_telemetry_disabled
  假设 `CHUB_TELEMETRY=0`
  当 调用 `chub_feedback`
  那么 返回 skipped 状态和 telemetry_disabled 原因

场景: `chub://registry` 返回简化 registry 摘要
  测试: mcp_resource_registry_returns_simplified_entries
  假设 registry 已加载
  当 客户端读取 `chub://registry`
  那么 返回 JSON 文本，包含 `entries`、`total` 和每个条目的公开摘要字段
