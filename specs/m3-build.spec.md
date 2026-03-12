spec: task
name: "m3-build"
inherits: chub-rs
---

## 意图

实现 `chub build <content-dir>`：扫描内容目录、发现 `DOC.md`/`SKILL.md`、解析 frontmatter、
生成 `registry.json` 和 `search-index.json`，并复制内容树到输出目录。这个阶段的目标是让 Rust
版本可以直接替代 JS 版本的内容构建流程和 fixture 构建流程。

## 决策

- 递归扫描作者目录中的 `DOC.md` 和 `SKILL.md`，并以顶层目录名作为 author。
- 如果作者目录中存在 `registry.json`，优先直接使用该索引，而不是继续自动发现。
- doc 条目的 `metadata.languages` 与 `metadata.versions` 都是必填；skill 不要求这两个字段。
- `search-index.json` 的结构继续保持 `version`、`algorithm`、`params`、`idf`、`documents` 等字段。
- `--validate-only` 只做校验和摘要输出，不写任何文件。
- `-o/--output` 默认为 `<content-dir>/dist`，并保持内容树的相对路径不变。
- 相同输入目录在相同配置下必须产出确定性的 `registry.json` 和 `search-index.json`；条目、文件和索引文档顺序不能依赖文件系统遍历顺序。

## 边界

### 允许修改
- `src/commands/build.rs`
- `src/lib/frontmatter.rs`
- `src/lib/bm25.rs`
- `tests/**`

### 禁止做
- 不要在这个任务里改写 search/get/update 的运行时行为。
- 不要引入与现有 content repo 结构不兼容的新 frontmatter 规范。

## 验收标准

场景: 构建最小内容目录会生成 registry 和 search index
  测试: build_minimal_content_dir
  假设 fixture 目录中只有一个包含 `DOC.md` 的 author 目录
  当 运行 `chub build <content-dir> -o <dist-dir>`
  那么 输出目录中生成 `registry.json`、`search-index.json` 和复制后的内容文件

场景: frontmatter 解析提取必需字段
  测试: frontmatter_parses_required_doc_fields
  假设 `DOC.md` 含有 `name`、`description` 和 `metadata.languages/versions/source/tags`
  当 解析 frontmatter
  那么 正确提取 name、description、languages、versions、tags 和 source

场景: 多作者多语言内容会按 `author/name` 分组
  测试: build_groups_multi_author_and_language_entries
  假设 fixture 中存在多个 author 目录和多语言 doc
  当 运行构建
  那么 `registry.json` 中 doc 的 `id` 采用 `author/name` 形式，并把不同语言聚合到同一条目下

场景: skill 条目被正确发现
  测试: build_discovers_skills
  假设 fixture 中存在 `SKILL.md`
  当 运行构建
  那么 `registry.json.skills` 中包含对应 skill 条目和文件列表

场景: 作者自定义 registry 优先于自动发现
  测试: build_uses_author_registry_when_present
  假设 某 author 目录包含 `registry.json`
  当 运行构建
  那么 构建过程直接采用该作者的 registry，并按 author 前缀修正相对路径

场景: 重复 ID 会返回构建错误
  测试: build_rejects_duplicate_ids
  假设 两个条目最终会生成相同的 `author/name`
  当 运行 `chub build`
  那么 进程失败，并在错误中指出重复 ID

场景: 缺少必需 frontmatter 字段会返回构建错误
  测试: build_rejects_missing_required_fields
  假设 某个 doc 缺少 `metadata.languages` 或 `metadata.versions`
  当 运行 `chub build`
  那么 进程失败，并指出缺失字段

场景: `--validate-only` 只输出摘要不写文件
  测试: build_validate_only_does_not_write_output
  假设 输入内容目录有效
  当 运行 `chub build <content-dir> --validate-only`
  那么 命令输出 docs/skills/warnings 摘要，并且不会创建输出目录

场景: BM25 索引结构与现有字段保持兼容
  测试: build_bm25_index_matches_expected_shape
  假设 构建输入包含多个 doc/skill 条目
  当 读取生成的 `search-index.json`
  那么 文件包含 `version`、`algorithm`、`params`、`totalDocs`、`avgFieldLengths`、`idf` 和 `documents`

场景: JSON 模式输出构建摘要
  测试: build_json_output_summary
  假设 输入内容目录有效
  当 运行 `chub build <content-dir> --validate-only --json`
  那么 输出 JSON 摘要，至少包含 `docs`、`skills`、`warnings` 和 `output`

场景: 相同输入重复构建产物保持确定性
  测试: build_outputs_are_deterministic_for_same_input
  假设 相同的内容目录在两次构建之间没有发生变化
  当 分别输出到两个不同的临时目录并比较生成的 `registry.json` 与 `search-index.json`
  那么 两次构建产物字节级一致，并且条目与文档顺序稳定
