spec: task
name: "m0-skeleton"
inherits: chub-rs
---

## 意图

建立一个可编译、可运行的 Rust CLI 骨架：定义 `chub` 的全局参数和全部一级子命令，
实现配置加载和统一输出模块，并为后续任务提供稳定的目录结构。这个阶段只要求命令壳、
默认配置和输出/错误行为对齐，不实现 search/get 等业务逻辑。

## 决策

- `src/main.rs` 负责全局参数解析、默认 usage 和顶层错误退出码。
- `src/commands/mod.rs` 定义全部一级子命令和 stub handler；未实现命令返回统一的“not implemented”错误。
- 配置读取使用 `OnceLock` 做进程内缓存。
- `load_config_inner()` 保留为纯函数，便于单元测试覆盖默认值和环境变量分支。
- 输出层统一提供“结构化 JSON”与“人类可读文本”两种模式，错误输出也走同一层。

## 边界

### 允许修改
- `Cargo.toml`
- `src/main.rs`
- `src/commands/**`
- `src/lib/config.rs`
- `src/lib/output.rs`
- `src/lib/mod.rs`
- `tests/**`

### 禁止做
- 不要在这个任务里实现真实的 registry、搜索、下载或 MCP 协议逻辑。
- 不要修改上游 JS CLI 或其 fixture。

## 验收标准

场景: 无子命令时显示自定义 usage
  测试: cli_no_args_prints_usage
  假设 已编译 `chub` 二进制
  当 直接运行 `chub`
  那么 输出自定义 usage，而不是直接 panic 或返回 clap 默认错误

场景: 帮助信息列出全部一级子命令和全局 JSON 标志
  测试: cli_help_shows_all_commands
  假设 已编译 `chub` 二进制
  当 运行 `chub --help`
  那么 输出包含 `search`、`get`、`annotate`、`feedback`、`update`、`cache`、`build` 和 `--json`

场景: 版本号别名输出与包版本一致
  测试: cli_version_aliases_match_package_version
  假设 `Cargo.toml` 中声明了包版本
  当 分别运行 `chub -V` 和 `chub --cli-version`
  那么 两个命令都输出 `chub x.y.z`

场景: 缺失配置文件时返回默认配置
  测试: config_defaults_match_js
  假设 `CHUB_DIR` 指向一个不存在 `config.yaml` 的临时目录
  当 调用 `load_config_inner()`
  那么 返回默认 source、`.context`、`21600` 秒刷新间隔和启用 telemetry 的配置

场景: 多 source 配置文件被正确解析
  测试: config_parses_multi_source_file
  假设 临时目录中的 `config.yaml` 同时声明 remote source 和 local source
  当 调用 `load_config_inner()`
  那么 返回的 source 列表保留每个 source 的 `name` 与 `url/path`

场景: `CHUB_DIR` 环境变量覆盖默认目录
  测试: config_chub_dir_env_override
  假设 已设置 `CHUB_DIR=/tmp/test-chub`
  当 调用 `get_chub_dir()`
  那么 返回 `/tmp/test-chub`

场景: JSON 输出模式返回结构化数据
  测试: output_json_mode_serializes_payload
  假设 输出层收到可序列化数据和 `json=true`
  当 调用输出函数
  那么 stdout 输出合法 JSON，且错误场景输出带 `error` 字段的 JSON 对象

场景: 人类模式调用专用格式化函数
  测试: output_human_mode_calls_formatter
  假设 输出层收到可序列化数据和 `json=false`
  当 调用输出函数
  那么 不输出 JSON，而是执行传入的人类格式化闭包
