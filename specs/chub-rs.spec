spec: project
name: "chub-rs"
---

## 意图

以 `/Users/zhangalex/Work/Projects/FW/rust-agents/context-hub/cli` 的当前实现和测试为兼容基线，
用 Rust 重写 `@aisuite/chub`，产出 `chub` 和 `chub-mcp` 两个二进制。优先保证命令、参数、
JSON 输出、持久化文件格式和本地缓存行为与现有 JS CLI 对齐，然后再收敛到更小的分发体积、
更快的启动时间和更低的资源占用。

## 约束

- 兼容性基线来自 `context-hub/cli/src/**`、`context-hub/cli/test/**`、`context-hub/cli/tests/**`；
  当 README、设计文档和测试冲突时，以当前 CLI 实现和测试断言为准。
- 必须提供 `chub` 和 `chub-mcp` 两个二进制，并保持现有命令名、工具名和资源名不变。
- 必须兼容现有 `registry.json`、`search-index.json`、`~/.chub/config.yaml` 和 annotation JSON 文件格式。
- 必须保留 `CHUB_DIR`、`CHUB_BUNDLE_URL`、`CHUB_TELEMETRY` 的行为和优先级。
- 必须保持 30 秒的 registry/doc/bundle HTTP 超时，以及 3 秒的 feedback/telemetry HTTP 超时。
- 每个迁移过来的外部行为都必须绑定 Rust 测试；优先移植现有 JS fixture 和断言，而不是重新发明更弱的测试。
- 测试必须在离线环境中可重复运行。
- 涉及 HTTP 的场景必须使用本地测试替身，不能依赖真实网络或外部服务可用性。
- 测试和实现都不得读写真实 `HOME`、真实 `~/.chub` 或调用者环境中的持久状态；文件副作用必须通过临时目录或可注入路径隔离。
- 凡是时间、刷新间隔、时间戳和超时判定逻辑，都必须通过可注入时钟或显式测试参数控制；不要把 `SystemTime::now()`、系统时区或真实 sleep 直接写进测试断言。
- `src/commands/**` 只负责参数解析、调用和输出组装；业务逻辑、I/O 编排和协议细节必须落在可单测的库层接口中。
- `--json` 模式下 stdout 只能输出契约定义的 JSON；人类模式的错误、警告和诊断信息必须写入 stderr。
- `registry.json`、`search-index.json`、annotation JSON 和关键 CLI/MCP JSON 输出都必须有 fixture 或 golden 测试做兼容性比较。
- 非测试代码必须通过 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings` 和 `cargo test`。
- 非测试代码中禁止使用 `unwrap`、`expect`、`panic!`、`todo!`、`unimplemented!` 作为常规控制流。
- 不要为了让 Rust 版本“更容易实现”而修改上游 JS 实现、fixture 或 spec 的兼容性目标。
- 不要要求 Node.js 作为 Rust 版本的安装时或运行时依赖。

## 决策

- CLI 参数解析使用 `clap` v4 derive。
- HTTP 客户端使用 `reqwest` + `rustls`，避免 OpenSSL 运行时依赖。
- JSON 和 YAML 分别使用 `serde_json` 与 `serde_yaml`。
- tar.gz bundle 解压使用 `flate2` + `tar`。
- 客户端 ID 继续使用机器标识的 SHA-256 哈希，保持现有匿名标识语义。
- 目录结构维持 `src/commands/**`、`src/lib/**`、`src/mcp/**` 的分层，命令层只做参数解析、调用和输出组装。
- 兼容性测试优先复用 `context-hub/cli/test/fixtures`、现有 e2e 断言和 MCP handler 断言。
- 新增第三方依赖前，先在对应 task spec 的 `## 决策` 中记录原因、替代方案和兼容性影响。

## 边界

### 允许修改
- `Cargo.toml`
- `.gitignore`
- `src/**`
- `tests/**`
- `specs/**`

### 禁止做
- 不要修改 `/Users/zhangalex/Work/Projects/FW/rust-agents/context-hub/**` 中的源码、测试或 fixture。
- 不要修改现有 registry/frontmatter/annotation 文件格式来适配 Rust 实现。

## 排除范围

- 新增 JS 版本当前不存在的用户可见功能。
- 在缺少兼容性测试前先做性能微优化或大规模架构重构。
- 修改远端服务协议、CDN 目录布局或内容仓库规范。

## 验收标准

场景: 兼容性测试矩阵覆盖当前公开行为
  测试: project_ported_parity_matrix_covers_public_surface
  假设 已读取 `context-hub/cli/src/**`、`context-hub/cli/test/**` 和 `context-hub/cli/tests/**`
  当 建立 Rust 版本的迁移测试矩阵
  那么 该矩阵覆盖 search、get、annotate、feedback、update、cache、build 和 MCP tools 的现有公开行为

场景: 两个对外二进制保持稳定名称
  测试: project_builds_chub_and_chub_mcp_bins
  假设 Cargo 清单已声明全部可执行目标
  当 构建当前工作区
  那么 产物中包含 `chub` 与 `chub-mcp` 两个二进制，并保持现有命令名、工具名和资源名

场景: Rust 版本可读取现有持久化格式
  测试: project_reads_existing_persistent_formats
  假设 存在由 JS 版本生成的 registry、search index、config 和 annotation fixture
  当 Rust 版本读取这些文件
  那么 无需迁移即可完成解析，并且关键字段与 JS 行为一致

场景: 环境变量和超时策略保持一致
  测试: project_preserves_env_precedence_and_timeouts
  假设 已设置 `CHUB_DIR`、`CHUB_BUNDLE_URL`、`CHUB_TELEMETRY` 且网络客户端可被测试替身观测
  当 运行涉及配置、下载和反馈的命令
  那么 环境变量优先级与 JS 版本一致，并使用 30 秒和 3 秒的超时策略

场景: 测试在离线隔离环境中可重复运行
  测试: project_tests_run_offline_and_in_isolated_dirs
  假设 测试环境禁用真实网络访问，并为 `HOME` 与 `CHUB_DIR` 提供临时目录
  当 运行仓库测试套件
  那么 测试在离线环境中可重复运行，涉及 HTTP 的场景只连接本地测试替身，并且不会访问真实 `~/.chub`、真实用户配置或外部网络
  并且 测试必须在离线环境中可重复运行

场景: 时间相关逻辑由测试显式控制
  测试: project_time_dependent_logic_is_test_controlled
  假设 刷新间隔、annotation 时间戳和超时判定测试都提供固定时钟或显式测试参数
  当 运行这些时间相关测试
  那么 结果只依赖注入的时间输入，不依赖 `SystemTime::now()`、系统时区或真实 sleep

场景: 命令层只做参数解析和输出组装
  测试: project_command_handlers_delegate_to_library_layers
  假设 代表性的 `search`、`get`、`update` 和 `build` handler 都可在测试中直接调用
  当 检查命令层与库层之间的调用边界
  那么 `src/commands/**` 只负责参数解析、调用和输出组装，而业务逻辑与 I/O 编排可通过库层接口单独验证

场景: JSON stdout 契约不被诊断信息污染
  测试: project_json_stdout_contracts_are_clean
  假设 代表性的 CLI JSON 命令和 MCP 请求都在测试中捕获 stdout/stderr
  当 运行 `--json` 模式命令和 MCP server 响应路径
  那么 stdout 只包含契约规定的 JSON 负载，而错误、警告和调试信息全部写入 stderr

场景: 非测试代码不包含快捷失败控制流
  测试: project_no_shortcut_failures_in_prod_code
  假设 当前仓库源码已完成实现
  当 运行静态代码质量检查
  那么 非测试代码中不存在 `unwrap`、`expect`、`panic!`、`todo!` 或 `unimplemented!`

场景: 仓库质量门禁全部通过
  测试: project_workspace_quality_gates
  假设 当前工作树包含可编译的 Rust 实现和测试
  当 运行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings` 和 `cargo test`
  那么 三个命令全部通过
