spec: task
name: "m1-core-read"
inherits: chub-rs
---

## 意图

实现 Context Hub 的核心读路径：加载并合并 registry、执行搜索、解析 doc/skill 条目、
获取内容并输出 `search` 与 `get` 命令结果。这个阶段的目标是先跑通用户和 agent 最常用的
“查找条目 -> 获取内容”工作流，并对齐当前 JS CLI 的外部行为。

## 决策

- registry 合并后保留 `_source` 与 `_sourceObj` 元信息，用于多源歧义提示和缓存读取。
- `search <exact-id>` 返回条目详情对象本身，而不是 `{ results: [...] }` 包装结构。
- `get` 对 doc 条目始终要求显式 `--lang`，包括单语言 doc；这是当前 JS 行为基线。
- `get --json` 仅在存在引用文件时输出 `additionalFiles`，仅在存在注解时输出 `annotation`。
- doc 的默认版本取 `recommendedVersion`；显式 `--version` 时必须严格匹配版本字符串。
- `get --file` 只允许获取条目声明过的文件列表中的路径，不做模糊匹配。
- `get --file` 在查找文件前必须先拒绝绝对路径、`..` 片段和规范化后越出条目根目录的路径。
- 远端 doc 的读取顺序为：local source -> 本地缓存 -> 远端下载。
  （chub-rs 不实现 npm bundled dist 兜底路径，这是 JS 特有的包分发机制。）
- `get -o` 多 ID 时合并所有内容用 `\n\n---\n\n` 分隔写入单文件；`-o dir/` 结尾时逐个写 `<dir>/<id>.md`。
- `get --full -o <dir>` 单 entry 直接写到 output 目录；多 entry 才嵌套 `<output>/<id>/`。
- 搜索优先使用预构建 BM25 index；缺失 index 时退回关键字打分搜索。

## 边界

### 允许修改
- `src/commands/search.rs`
- `src/commands/get.rs`
- `src/lib/registry.rs`
- `src/lib/cache.rs`
- `src/lib/bm25.rs`
- `src/lib/normalize.rs`
- `src/lib/frontmatter.rs`
- `src/lib/annotations.rs`
- `tests/**`

### 禁止做
- 不要在这个任务里写 annotation/feedback/update/cache 的写路径逻辑。
- 不要修改 `registry.json` 或 `search-index.json` 的现有字段语义。

## 验收标准

场景: 无查询时列出全部条目
  测试: search_lists_all_entries
  假设 已加载包含 doc 和 skill 的 registry fixture
  当 运行 `chub search`
  那么 返回所有可见条目，并在 JSON 模式下输出 `{ results, total }`

场景: 精确 ID 查询返回条目详情
  测试: search_exact_id_returns_entry_detail
  假设 registry 中存在 `acme/widgets`
  当 运行 `chub search acme/widgets --json`
  那么 返回条目详情对象本身，包含 `id`、`languages`、`tags` 等字段，而不是 `results` 包装

场景: 模糊搜索支持 BM25 和过滤条件
  测试: search_fuzzy_ranking_and_filters
  假设 已加载 search index 和多条带 tag/lang 的 registry 条目
  当 运行 `chub search widget --tags automation --lang js`
  那么 结果按相关度排序，并只保留满足 tag/lang 过滤的条目

场景: 获取 doc 内容时返回入口文件和附加文件提示
  测试: get_doc_entry_and_additional_files
  假设 `acme/widgets` 是包含 `DOC.md` 与 `references/advanced.md` 的 doc 条目
  当 运行 `chub get acme/widgets --lang js`
  那么 输出 `DOC.md` 内容，并追加 `additionalFiles` 提示而不是直接输出全部文件

场景: doc 缺少 `--lang` 时返回错误
  测试: get_requires_lang_for_docs
  假设 registry 中存在单语言 doc `acme/widgets` 和多语言 doc `multilang/client`
  当 分别运行 `chub get acme/widgets` 和 `chub get multilang/client`
  那么 两个命令都失败，并提示可用语言或要求显式提供 `--lang`

场景: skill 条目无需语言参数
  测试: get_skill_content_without_lang
  假设 registry 中存在 skill 条目 `testskills/deploy`
  当 运行 `chub get testskills/deploy`
  那么 输出 `SKILL.md` 内容且不会要求 `--lang`

场景: 默认版本和显式版本解析正确
  测试: get_recommended_and_specific_versions
  假设 `acme/versioned-api` 的 `javascript` 语言包含 `2.0.0` 和 `1.0.0`
  当 分别运行 `chub get acme/versioned-api --lang js` 和 `chub get acme/versioned-api --lang js --version 1.0.0`
  那么 第一个命令返回推荐版本内容，第二个命令返回 `1.0.0` 的内容

场景: 不存在的版本返回可选版本列表
  测试: get_missing_version_lists_available_versions
  假设 `acme/versioned-api` 的 `javascript` 语言只有 `2.0.0` 和 `1.0.0`
  当 运行 `chub get acme/versioned-api --lang js --version 99.0.0`
  那么 命令失败，并在错误中列出 `2.0.0` 和 `1.0.0`

场景: `--file` 获取指定文件并拒绝未知文件
  测试: get_specific_file_and_missing_file_error
  假设 `acme/widgets` 的 `references/advanced.md` 已出现在条目文件列表中
  当 分别运行 `chub get acme/widgets --lang js --file references/advanced.md` 和 `chub get acme/widgets --lang js --file nonexistent.md`
  那么 第一个命令只输出指定文件内容，第二个命令失败并列出可用文件

场景: `--file` 拒绝路径穿越和绝对路径
  测试: get_rejects_path_traversal_in_file_flag
  假设 `acme/widgets` 条目根目录之外存在其他文件，且条目自身声明了 `references/advanced.md`
  当 分别运行 `chub get acme/widgets --lang js --file ../secrets.md`、`chub get acme/widgets --lang js --file /tmp/secrets.md` 和 `chub get acme/widgets --lang js --file references/../../DOC.md`
  那么 三个命令都在访问磁盘前失败，并明确提示 `--file` 必须匹配条目声明过的相对路径

场景: 多 source 冲突时要求 `source:id`
  测试: get_ambiguous_id_lists_source_alternatives
  假设 两个 source 都声明了 `openai/chat`
  当 运行 `chub get openai/chat`
  那么 命令失败，并列出 `source:id` 形式的可选项

场景: `-o` 写文件且 JSON 模式返回 metadata 不含 content
  测试: get_output_flag_writes_file_not_stdout
  假设 `acme/widgets` 的缓存内容已就绪
  当 运行 `chub get acme/widgets --lang js -o out.md --json`
  那么 文件 `out.md` 包含 doc 内容，JSON 输出包含 `{id, type, path}` 而不含 `content` 字段

场景: 多 ID `-o` 合并内容写入单文件
  测试: get_multi_id_output_combines_content
  假设 `acme/widgets` 和 `testskills/deploy` 的缓存内容已就绪
  当 运行 `chub get acme/widgets testskills/deploy --lang js -o combined.md`
  那么 `combined.md` 包含两个条目的内容，用 `---` 分隔，而不是只有最后一个条目

场景: 单 entry `--full -o` 直接写到输出目录
  测试: get_full_output_single_entry_writes_directly_to_dir
  假设 `acme/widgets` 包含 `DOC.md` 和 `references/advanced.md`
  当 运行 `chub get acme/widgets --lang js --full -o output/`
  那么 文件直接写到 `output/` 目录，不嵌套 `output/acme/widgets/` 子目录

场景: JSON 输出按需包含注解和附加文件字段
  测试: get_json_includes_optional_annotation_and_additional_files
  假设 `acme/widgets` 同时具有 annotation 和附加文件，而 `multilang/client` 两者都没有
  当 分别运行两个条目的 `chub get ... --json`
  那么 前者 JSON 包含 `annotation` 与 `additionalFiles`，后者两个字段都省略
