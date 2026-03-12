spec: task
name: "fix-review-findings"
inherits: chub-rs
---

## 意图

修复代码审查发现的运行时缺陷：local source path 未加载、HTTP 非 2xx 响应写入缓存、
空 versions 数组导致 panic、多源 BM25 搜索分数覆盖、stale cache 未检查 registry.json
存在性、全失败 `-o` 创建空文件。这些问题源于原始 spec 的验收标准遗漏——决策写了但未
绑定测试场景、error path 缺失、全局约束未具体化。

## 决策

- `load_merged_registry` 和 `chub_mcp` 在加载 registry 时，优先从 `source.path`
  读取 `registry.json`，仅在本地路径不存在时回退到 `~/.chub/sources/` 缓存。
- `get` 命令通过 `source_paths` 映射传递 local path 给 `fetch_doc`，不再硬编码 `None`。
- `update` 的 registry 和 bundle HTTP 请求必须调用 `error_for_status()`，
  4xx/5xx 响应必须返回错误，不得写入缓存或更新 meta.json。
- `resolve_doc_path` 遇到空 versions 数组时返回 `Unresolvable`，而不是 index panic。
- 多源 BM25 搜索结果按 ID 取最高分，不允许后注册的源覆盖先前源的分数。
- `is_cache_fresh()` 除了检查 `meta.last_updated` 时间戳外，还必须验证 `registry.json`
  文件确实存在；meta 新鲜但文件缺失时视为 stale，触发重新下载。
- `get -o` 在所有 entry 都失败时（`results` 为空）不创建或截断输出文件；
  仅在 JSON 模式下输出错误 JSON 到 stdout。

## 边界

### 允许修改
- `src/commands/mod.rs`
- `src/commands/get.rs`
- `src/commands/update.rs`
- `src/core/cache.rs`
- `src/core/registry.rs`
- `src/bin/chub_mcp.rs`
- `tests/**`

### 禁止做
- 不要修改现有通过的测试的断言语义。
- 不要引入新的第三方依赖。

## 验收标准

场景: local source 的 registry 直接从 source.path 加载
  测试: local_source_loads_registry_from_path
  假设 配置了一个 local source，其 `path` 指向包含 `registry.json` 的临时目录
  当 调用 `load_merged_registry()` 或初始化 MCP context
  那么 返回的 entries 包含 local source 的条目，且不需要预先在 `~/.chub/sources/` 缓存

场景: get 命令通过 local source path 读取内容
  测试: get_reads_content_from_local_source_path
  假设 配置了 local source 且 source.path 下存在 doc 内容文件
  当 运行 `chub get <id> --lang <lang>`
  那么 内容直接从 source.path 读取，不从缓存目录读取

场景: update 拒绝 HTTP 4xx/5xx 响应
  测试: update_rejects_non_2xx_registry_response
  假设 测试替身对 registry.json 请求返回 404 或 500
  当 运行 `chub update`
  那么 命令返回错误，且 `registry.json` 和 `meta.json` 不被写入或更新

场景: update --full 拒绝 HTTP 错误的 bundle 响应
  测试: update_rejects_non_2xx_bundle_response
  假设 测试替身对 bundle.tar.gz 请求返回 500
  当 运行 `chub update --full`
  那么 命令返回错误，且 `sources/<name>/data/` 目录不被解压覆盖

场景: resolve_doc_path 对空 versions 数组返回 Unresolvable
  测试: resolve_doc_path_empty_versions_returns_unresolvable
  假设 registry 中存在一个 doc 条目，其某个语言的 versions 数组为空
  当 调用 `resolve_doc_path(entry, Some("lang"), None)`
  那么 返回 `ResolvedPath::Unresolvable`，而不是 panic

场景: ensure_registry 在 CLI 和 MCP 两个入口都被调用
  测试: ensure_registry_called_in_both_entry_points
  假设 CLI 入口 `commands/mod.rs` 和 MCP 入口 `chub_mcp.rs` 都包含 ensure_registry 调用
  当 检查两个入口文件的源码
  那么 两个文件都包含 `ensure_registry(` 调用，确保远程 registry 在启动时自动引导

场景: update.rs 对 HTTP 响应调用 error_for_status
  测试: update_rs_uses_error_for_status_on_http_responses
  假设 `update.rs` 中至少有 2 处 HTTP 请求（registry + bundle）
  当 检查源码中 `.error_for_status()?` 的出现次数
  那么 至少出现 2 次，确保 4xx/5xx 响应不会静默写入缓存

场景: cache.rs 对远程文件获取调用 error_for_status
  测试: cache_rs_uses_error_for_status_on_remote_fetch
  假设 `cache.rs` 中存在 on-demand 远程文件获取路径
  当 检查源码中 `.error_for_status()?` 的出现
  那么 确认存在，确保 CDN 返回错误时不会缓存无效内容

场景: 多源 BM25 搜索对重复 ID 取最高分
  测试: multi_source_bm25_takes_max_score_for_duplicate_ids
  假设 两个 source 都注册了 ID 为 `openai/chat` 的条目，各自 BM25 分数不同
  当 执行 `search_entries("openai", merged, filters)`
  那么 结果中 `openai/chat` 只出现一次，且 `_score` 是两个源中的较大值

场景: is_cache_fresh 在 registry.json 缺失时返回 stale
  测试: is_cache_fresh_returns_false_when_registry_json_missing
  假设 远程 source 的 `meta.json` 记录了新鲜的 `last_updated` 时间戳
  但是 `sources/<name>/registry.json` 文件不存在（被误删或部分清理）
  当 调用 `is_cache_fresh(chub_dir, source_name, interval, now)`
  那么 返回 `false`，触发 `ensure_registry` 重新下载

场景: is_cache_fresh 在 meta 和 registry.json 都存在时返回 true
  测试: is_cache_fresh_returns_true_when_both_meta_and_registry_exist
  假设 远程 source 的 `meta.json` 和 `registry.json` 都存在且时间戳新鲜
  当 调用 `is_cache_fresh(chub_dir, source_name, interval, now)`
  那么 返回 `true`

场景: get -o 全失败时不创建输出文件
  测试: get_rs_guards_empty_results_before_output_to_file
  假设 所有请求的 entry ID 都解析失败（results 为空）
  当 检查 `get.rs` 源码中的 output 逻辑
  那么 确认存在 `results.is_empty()` 守卫，跳过 `output_to_file` 调用
