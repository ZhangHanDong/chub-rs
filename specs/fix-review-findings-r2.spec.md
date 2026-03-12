spec: task
name: "fix-review-findings-r2"
inherits: chub-rs
---

## 意图

修复第二轮代码审查发现的三个问题：MCP 多源搜索索引覆盖、本地源 cache fallback
与 JS 行为不一致、HTTP 状态码测试覆盖名不副实。

## 决策

- `chub_mcp.rs` 必须先收集所有 source_data 到 Vec，再调用一次 `merge_registries`，
  禁止逐源合并覆盖 `search_index`。
- 本地源（`source.path.is_some()`）在加载 registry 后必须 `continue`，
  不得 fallback 到 `~/.chub/sources/` 缓存路径，与 JS 基线行为一致。
- 测试名必须准确反映测试内容：cache 层错误传播测试不得命名为 "rejects non-2xx"，
  需通过 `run_with_fetcher` 补充命令层错误传播测试。

## 边界

### 允许修改
- `src/bin/chub_mcp.rs`
- `src/commands/mod.rs`
- `tests/fix_review_findings.rs`

### 禁止做
- 不要修改现有通过测试的断言语义。
- 不要引入新的第三方依赖。

## 验收标准

场景: MCP 多源搜索使用单次合并
  测试: mcp_tool_search_returns_simplified_results
  假设 `chub_mcp.rs` 配置了多个远程源
  当 初始化 MCP context
  那么 所有 source_data 先收集到 Vec 再调用一次 `merge_registries`
  并且 search_index 包含所有源的条目，不被最后一个源覆盖

场景: 本地源不 fallback 到缓存
  测试: local_source_loads_registry_from_path
  假设 配置了 local source 且 source.path 存在
  当 加载 merged registry（CLI 或 MCP 入口）
  那么 local source 直接从 source.path 读取 registry.json
  并且 不检查 `~/.chub/sources/` 缓存目录

场景: 测试名准确反映覆盖范围
  测试: cache_layer_propagates_registry_fetch_error
  假设 cache 层的 `fetch_and_save_registry` 接收到返回错误的闭包
  当 执行该函数
  那么 错误被传播，registry.json 和 meta.json 不被写入

场景: update 命令层传播 fetcher 错误
  测试: update_command_propagates_fetcher_errors
  假设 通过 `run_with_fetcher` 注入返回 HTTP 404 错误的 fetcher
  当 运行 update 命令
  那么 错误被收集，registry.json 不被创建
