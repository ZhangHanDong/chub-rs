spec: task
name: "m2-write-path"
inherits: chub-rs
---

## 意图

实现 Context Hub 的写路径：annotation 的持久化与读取、cache 统计与清理、registry/bundle 更新、
feedback 发送、client id 生成和 analytics。这个阶段覆盖“代理在使用后记住经验、刷新数据、
向维护者发送反馈”的闭环。

## 决策

- annotation 文件保存在 `~/.chub/annotations/<safe-id>.json`，其中 `<safe-id>` 用 `--` 替换 `/`。
- `cache clear` 只保留 `config.yaml`，其余缓存目录全部删除。
- `update --full` 下载 `bundle.tar.gz` 并解压到 `~/.chub/sources/<name>/data/`。
- `update` 默认遵守 `refresh_interval`，只有 `--force` 时才跳过 freshness 检查；这显式修复当前 JS 中总是强制刷新的实现缺陷。
- feedback 保持当前 payload 语义，并支持 `--label`、`--lang`、`--doc-version`、`--file`、`--agent`、`--model` 和 `--status`。
- telemetry 被禁用时，feedback 和 analytics 必须返回或结束为 `skipped`/no-op，而不是抛错。
- client id 继续使用机器标识的 SHA-256 十六进制字符串，并缓存到 `~/.chub/client_id`。

## 边界

### 允许修改
- `src/commands/annotate.rs`
- `src/commands/feedback.rs`
- `src/commands/update.rs`
- `src/commands/cache.rs`
- `src/lib/annotations.rs`
- `src/lib/cache.rs`
- `src/lib/telemetry.rs`
- `src/lib/identity.rs`
- `src/lib/analytics.rs`
- `tests/**`

### 禁止做
- 不要在这个任务里引入新的 registry/search/build 结构或 MCP 协议层。
- 不要让 analytics 或 feedback 阻塞主命令的成功路径。

## 验收标准

场景: annotation 支持写入、读取、替换和清除
  测试: annotate_crud_round_trip
  假设 `CHUB_DIR` 指向临时目录
  当 依次写入 annotation、读取 annotation、再次写入新 note、再执行 `chub annotate acme/widgets --clear`
  那么 note 被持久化、再次读取时可见、重新写入会替换旧值，清除后文件被删除

场景: annotation 列表返回全部持久化注解
  测试: annotate_list_returns_all_annotations
  假设 临时目录中存在多个 annotation 文件
  当 运行 `chub annotate --list --json`
  那么 返回 annotation 数组，包含每条记录的 `id`、`note` 和 `updatedAt`

场景: `get` 会在文档末尾附加 annotation
  测试: annotation_is_appended_on_get
  假设 某 doc 条目存在 annotation
  当 运行 `chub get <id> --lang js`
  那么 文档正文后追加 `[Agent note — timestamp]` 和 note 内容

场景: `cache status` 输出每个 source 的缓存摘要
  测试: cache_status_reports_sources_and_sizes
  假设 `~/.chub/sources/` 下存在 remote cache 和 local source 配置
  当 运行 `chub cache status --json`
  那么 返回每个 source 的类型、registry 状态、lastUpdated、fileCount 和 dataSize

场景: `cache clear` 只保留 `config.yaml`
  测试: cache_clear_preserves_config_only
  假设 `CHUB_DIR` 下同时存在 `config.yaml`、annotations 和 sources cache
  当 运行 `chub cache clear`
  那么 `config.yaml` 被保留，其余缓存与数据目录被删除

场景: `update` 下载 registry 并遵守 freshness/force
  测试: update_fetches_registry_with_refresh_policy
  假设 配置了 remote source，且测试替身可区分 fresh cache 与 stale cache
  当 分别运行普通 `chub update` 与 `chub update --force`
  那么 普通更新只在 cache 过期时下载，`--force` 总是重新下载并更新 `meta.json`

场景: `update --full` 下载并解压 bundle
  测试: update_full_bundle_downloads_and_extracts
  假设 配置了 remote source 且测试替身返回一个 `bundle.tar.gz`
  当 运行 `chub update --full`
  那么 bundle 被解压到 `sources/<name>/data/`，并同步更新 `registry.json`

场景: `feedback --status` 输出 telemetry 状态
  测试: feedback_status_reports_telemetry_configuration
  假设 telemetry 开关与 endpoint 已配置
  当 运行 `chub feedback --status --json`
  那么 返回 `telemetry`、`endpoint`、`client_id_prefix` 和 `valid_labels`

场景: telemetry 禁用时 feedback 返回 skipped
  测试: feedback_skips_when_telemetry_disabled
  假设 `CHUB_TELEMETRY=0` 或配置文件中 `telemetry=false`
  当 运行 `chub feedback openai/chat up`
  那么 命令不发送网络请求，并返回 skipped 状态

场景: feedback 请求携带解析后的上下文参数
  测试: feedback_payload_includes_labels_and_inferred_context
  假设 registry 中存在目标条目，且 HTTP 测试替身可记录请求体
  当 运行带 comment、label、lang、doc-version、file、agent 和 model 参数的 `chub feedback`
  那么 请求体包含 `entry_id`、`rating`、`comment`、`labels`、语言/版本/文件和 client id

场景: client id 首次创建后被缓存复用
  测试: client_id_is_created_and_cached
  假设 `~/.chub/client_id` 初始不存在
  当 连续两次调用 `get_or_create_client_id()`
  那么 第一次生成 64 字符十六进制字符串并写盘，第二次返回相同值

场景: analytics 在缺少依赖或 telemetry 禁用时不抛错
  测试: analytics_is_non_blocking
  假设 telemetry 被禁用或 analytics 后端未初始化
  当 调用 `track_event` 与 `shutdown_analytics`
  那么 调用成功返回，并且不会阻塞主流程或向上抛出异常
