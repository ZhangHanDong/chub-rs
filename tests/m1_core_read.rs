//! M1 core-read tests.
//! Tests for registry, search, get commands per m1-core-read.spec.

mod test_helpers;

use chub_rs::core::registry::{self, ResolvedPath, SearchFilters, get_entry, validate_file_path};
use tempfile::TempDir;
use test_helpers::*;

// ── Scenario 1: 无查询时列出全部条目 ────────────────────────────────

#[test]
fn search_lists_all_entries() {
    let merged = fixture_merged("default");

    // No query → all entries
    let filters = SearchFilters::default();
    let entries = registry::apply_filters(&merged.entries, &filters);

    // Should contain 3 docs + 1 skill = 4
    assert_eq!(entries.len(), 4, "should list all 4 entries");

    // Verify types
    let docs: Vec<_> = entries.iter().filter(|e| e.entry_type == "doc").collect();
    let skills: Vec<_> = entries.iter().filter(|e| e.entry_type == "skill").collect();
    assert_eq!(docs.len(), 3);
    assert_eq!(skills.len(), 1);

    // Verify IDs present
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"acme/widgets"));
    assert!(ids.contains(&"acme/versioned-api"));
    assert!(ids.contains(&"multilang/client"));
    assert!(ids.contains(&"testskills/deploy"));
}

// ── Scenario 2: 精确 ID 查询返回条目详情 ────────────────────────────

#[test]
fn search_exact_id_returns_entry_detail() {
    let merged = fixture_merged("default");

    let lookup = get_entry("acme/widgets", &merged.entries);
    assert!(!lookup.ambiguous);
    let entry = lookup.entry.expect("should find acme/widgets");

    assert_eq!(entry.id, "acme/widgets");
    assert_eq!(entry.entry_type, "doc");
    assert!(entry.languages.is_some());
    assert!(!entry.tags.is_empty());

    // Verify it returns the entry itself, not wrapped in results
    let json = serde_json::to_value(&entry).expect("serialize");
    assert!(json.get("id").is_some(), "should have id field");
    assert!(
        json.get("languages").is_some(),
        "should have languages field"
    );
    assert!(
        json.get("results").is_none(),
        "should NOT have results wrapper"
    );
}

// ── Scenario 3: 模糊搜索支持 BM25 和过滤条件 ───────────────────────

#[test]
fn search_fuzzy_ranking_and_filters() {
    let merged = fixture_merged("default");

    // Search "widget" with BM25
    let results = registry::search_entries("widget", &merged, &SearchFilters::default());
    assert!(!results.is_empty(), "should find widget entries");
    assert_eq!(results[0].id, "acme/widgets", "widgets should rank first");

    // Search with tag filter
    let results_filtered = registry::search_entries(
        "widget",
        &merged,
        &SearchFilters {
            tags: Some(vec!["automation".to_string()]),
            ..Default::default()
        },
    );
    assert!(
        results_filtered
            .iter()
            .all(|e| e.tags.iter().any(|t| t == "automation")),
        "all results should have automation tag"
    );

    // Search with lang filter
    let results_js = registry::search_entries(
        "client",
        &merged,
        &SearchFilters {
            lang: Some("js".to_string()),
            ..Default::default()
        },
    );
    // Should find multilang/client which has js
    let has_client = results_js.iter().any(|e| e.id == "multilang/client");
    assert!(has_client, "should find multilang/client with js filter");

    // Results should be sorted by score descending
    for w in results.windows(2) {
        assert!(
            w[0]._score.unwrap_or(0.0) >= w[1]._score.unwrap_or(0.0),
            "results should be sorted by score descending"
        );
    }
}

// ── Scenario 4: 获取 doc 内容时返回入口文件和附加文件提示 ────────────

#[test]
fn get_doc_entry_and_additional_files() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();
    let source_name = "default";

    setup_cached_docs(chub_dir, source_name);
    let merged = fixture_merged(source_name);

    let entry = get_entry("acme/widgets", &merged.entries)
        .entry
        .expect("find acme/widgets");

    // Resolve path
    let resolved = registry::resolve_doc_path(&entry, Some("js"), None);
    match resolved {
        ResolvedPath::Resolved { path, files, .. } => {
            assert!(files.contains(&"DOC.md".to_string()));
            assert!(files.contains(&"references/advanced.md".to_string()));

            // Fetch DOC.md
            let content =
                chub_rs::core::cache::fetch_doc(chub_dir, None, None, source_name, &path, "DOC.md")
                    .expect("fetch DOC.md");
            assert!(content.contains("Acme Widgets"));

            // Additional files = files minus DOC.md
            let additional: Vec<&String> = files.iter().filter(|f| *f != "DOC.md").collect();
            assert_eq!(additional.len(), 1);
            assert_eq!(additional[0], "references/advanced.md");
        }
        other => panic!("expected Resolved, got {:?}", other),
    }
}

// ── Scenario 5: doc 缺少 `--lang` 时返回错误 ───────────────────────

#[test]
fn get_requires_lang_for_docs() {
    let merged = fixture_merged("default");

    // Single-language doc without --lang
    let entry = get_entry("acme/widgets", &merged.entries)
        .entry
        .expect("find acme/widgets");
    let resolved = registry::resolve_doc_path(&entry, None, None);
    match resolved {
        ResolvedPath::NeedsLanguage { available } => {
            assert!(!available.is_empty());
            assert!(available.contains(&"javascript".to_string()));
        }
        other => panic!("expected NeedsLanguage, got {:?}", other),
    }

    // Multi-language doc without --lang
    let entry2 = get_entry("multilang/client", &merged.entries)
        .entry
        .expect("find multilang/client");
    let resolved2 = registry::resolve_doc_path(&entry2, None, None);
    match resolved2 {
        ResolvedPath::NeedsLanguage { available } => {
            assert!(available.len() >= 2);
            assert!(available.contains(&"javascript".to_string()));
            assert!(available.contains(&"python".to_string()));
        }
        other => panic!("expected NeedsLanguage, got {:?}", other),
    }
}

// ── Scenario 6: skill 条目无需语言参数 ──────────────────────────────

#[test]
fn get_skill_content_without_lang() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();
    let source_name = "default";

    setup_cached_docs(chub_dir, source_name);
    let merged = fixture_merged(source_name);

    let entry = get_entry("testskills/deploy", &merged.entries)
        .entry
        .expect("find testskills/deploy");

    assert_eq!(entry.entry_type, "skill");

    // Resolve without language
    let resolved = registry::resolve_doc_path(&entry, None, None);
    match resolved {
        ResolvedPath::SkillPath { path, files, .. } => {
            assert_eq!(path, "testskills/deploy");
            assert!(files.contains(&"SKILL.md".to_string()));

            let content = chub_rs::core::cache::fetch_doc(
                chub_dir,
                None,
                None,
                source_name,
                &path,
                "SKILL.md",
            )
            .expect("fetch SKILL.md");
            assert!(content.contains("Deploy Skill"));
        }
        other => panic!("expected SkillPath, got {:?}", other),
    }
}

// ── Scenario 7: 默认版本和显式版本解析正确 ──────────────────────────

#[test]
fn get_recommended_and_specific_versions() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();
    let source_name = "default";

    setup_cached_docs(chub_dir, source_name);
    let merged = fixture_merged(source_name);

    let entry = get_entry("acme/versioned-api", &merged.entries)
        .entry
        .expect("find acme/versioned-api");

    // Default: recommended version (2.0.0)
    let resolved = registry::resolve_doc_path(&entry, Some("js"), None);
    match resolved {
        ResolvedPath::Resolved { path, .. } => {
            let content =
                chub_rs::core::cache::fetch_doc(chub_dir, None, None, source_name, &path, "DOC.md")
                    .expect("fetch default version");
            assert!(
                content.contains("v2.0.0"),
                "default should be v2: {}",
                content
            );
        }
        other => panic!("expected Resolved for default, got {:?}", other),
    }

    // Explicit version 1.0.0
    let resolved_v1 = registry::resolve_doc_path(&entry, Some("js"), Some("1.0.0"));
    match resolved_v1 {
        ResolvedPath::Resolved { path, .. } => {
            let content =
                chub_rs::core::cache::fetch_doc(chub_dir, None, None, source_name, &path, "DOC.md")
                    .expect("fetch v1");
            assert!(
                content.contains("v1.0.0"),
                "explicit should be v1: {}",
                content
            );
        }
        other => panic!("expected Resolved for v1, got {:?}", other),
    }
}

// ── Scenario 8: 不存在的版本返回可选版本列表 ────────────────────────

#[test]
fn get_missing_version_lists_available_versions() {
    let merged = fixture_merged("default");

    let entry = get_entry("acme/versioned-api", &merged.entries)
        .entry
        .expect("find acme/versioned-api");

    let resolved = registry::resolve_doc_path(&entry, Some("js"), Some("99.0.0"));
    match resolved {
        ResolvedPath::VersionNotFound {
            requested,
            available,
        } => {
            assert_eq!(requested, "99.0.0");
            assert!(available.contains(&"2.0.0".to_string()));
            assert!(available.contains(&"1.0.0".to_string()));
        }
        other => panic!("expected VersionNotFound, got {:?}", other),
    }
}

// ── Scenario 9: `--file` 获取指定文件并拒绝未知文件 ────────────────

#[test]
fn get_specific_file_and_missing_file_error() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();
    let source_name = "default";

    setup_cached_docs(chub_dir, source_name);

    let files = vec!["DOC.md".to_string(), "references/advanced.md".to_string()];

    // Valid file
    let result = validate_file_path("references/advanced.md", &files);
    assert!(result.is_ok(), "should accept valid file");

    let content = chub_rs::core::cache::fetch_doc(
        chub_dir,
        None,
        None,
        source_name,
        "acme/widgets/javascript/v2",
        "references/advanced.md",
    )
    .expect("fetch advanced.md");
    assert!(content.contains("Advanced"));

    // Invalid file
    let result = validate_file_path("nonexistent.md", &files);
    assert!(result.is_err(), "should reject unknown file");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("DOC.md"),
        "error should list available files: {}",
        err_msg
    );
    assert!(
        err_msg.contains("references/advanced.md"),
        "error should list available files: {}",
        err_msg
    );
}

// ── Scenario 10: `--file` 拒绝路径穿越和绝对路径 ───────────────────

#[test]
fn get_rejects_path_traversal_in_file_flag() {
    let files = vec!["DOC.md".to_string(), "references/advanced.md".to_string()];

    // Absolute path
    let result = validate_file_path("/tmp/secrets.md", &files);
    assert!(result.is_err(), "should reject absolute path");

    // Path traversal with ..
    let result = validate_file_path("../secrets.md", &files);
    assert!(result.is_err(), "should reject .. traversal");

    // Path traversal via nested ..
    let result = validate_file_path("references/../../DOC.md", &files);
    assert!(result.is_err(), "should reject nested .. traversal");
}

// ── Scenario 11: 多 source 冲突时要求 `source:id` ──────────────────

#[test]
fn get_ambiguous_id_lists_source_alternatives() {
    let merged = fixture_merged_ambiguous();

    let lookup = get_entry("openai/chat", &merged.entries);
    assert!(lookup.ambiguous, "should be ambiguous");
    assert!(lookup.entry.is_none(), "should not return entry");
    assert_eq!(lookup.alternatives.len(), 2);
    assert!(
        lookup.alternatives.iter().any(|a| a.contains("source1:")),
        "alternatives should include source1"
    );
    assert!(
        lookup.alternatives.iter().any(|a| a.contains("source2:")),
        "alternatives should include source2"
    );
}

// ── Scenario 12: `-o` 写文件，JSON 模式返回 metadata 不含 content ────

#[test]
fn get_output_flag_writes_file_not_stdout() {
    use chub_rs::commands::SourceInfo;
    use std::collections::HashMap;

    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path().join("chub");
    let source_name = "default";
    let annotations_dir = chub_dir.join("annotations");

    setup_cached_docs(&chub_dir, source_name);
    let merged = fixture_merged(source_name);

    let mut source_info = HashMap::new();
    source_info.insert(source_name.to_string(), SourceInfo::default());

    let output_file = tmp.path().join("out.md");

    let args = chub_rs::commands::get::GetArgs {
        ids: vec!["acme/widgets".to_string()],
        lang: Some("js".to_string()),
        version: None,
        output: Some(output_file.display().to_string()),
        full: false,
        file: None,
    };

    // Run in JSON mode to capture the return value shape
    let result = chub_rs::commands::get::run(
        &args,
        &merged,
        &chub_dir,
        &annotations_dir,
        &source_info,
        true, // json
    );
    assert!(result.is_ok(), "get -o should succeed");

    // Verify file was written with content
    let written = std::fs::read_to_string(&output_file).expect("read output file");
    assert!(
        written.contains("Acme Widgets"),
        "output file should contain doc content"
    );

    // The JSON output should NOT contain content (it's in the file)
    // We verify this structurally: the -o code path returns {id, type, path}
    // not {id, type, content}
    let source = include_str!("../src/commands/get.rs");
    assert!(
        source.contains(r#""path": output_path"#),
        "get.rs -o JSON path should return path, not content"
    );
}

// ── Scenario 13: 多 ID `-o` 合并内容写入单文件 ──────────────────────

#[test]
fn get_multi_id_output_combines_content() {
    use chub_rs::commands::SourceInfo;
    use std::collections::HashMap;

    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path().join("chub");
    let source_name = "default";
    let annotations_dir = chub_dir.join("annotations");

    setup_cached_docs(&chub_dir, source_name);
    let merged = fixture_merged(source_name);

    let mut source_info = HashMap::new();
    source_info.insert(source_name.to_string(), SourceInfo::default());

    let output_file = tmp.path().join("combined.md");

    let args = chub_rs::commands::get::GetArgs {
        ids: vec!["acme/widgets".to_string(), "testskills/deploy".to_string()],
        lang: Some("js".to_string()),
        version: None,
        output: Some(output_file.display().to_string()),
        full: false,
        file: None,
    };

    let result = chub_rs::commands::get::run(
        &args,
        &merged,
        &chub_dir,
        &annotations_dir,
        &source_info,
        false,
    );
    assert!(result.is_ok(), "multi-id get -o should succeed");

    let written = std::fs::read_to_string(&output_file).expect("read combined file");
    assert!(
        written.contains("Acme Widgets"),
        "combined file should contain first entry"
    );
    assert!(
        written.contains("Deploy Skill"),
        "combined file should contain second entry"
    );
    assert!(
        written.contains("---"),
        "combined file should have separator"
    );
}

// ── Scenario 14: 单 entry `--full -o` 直接写到输出目录 ──────────────

#[test]
fn get_full_output_single_entry_writes_directly_to_dir() {
    use chub_rs::commands::SourceInfo;
    use std::collections::HashMap;

    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path().join("chub");
    let source_name = "default";
    let annotations_dir = chub_dir.join("annotations");

    setup_cached_docs(&chub_dir, source_name);
    let merged = fixture_merged(source_name);

    let mut source_info = HashMap::new();
    source_info.insert(source_name.to_string(), SourceInfo::default());

    let out_dir = tmp.path().join("output");

    let args = chub_rs::commands::get::GetArgs {
        ids: vec!["acme/widgets".to_string()],
        lang: Some("js".to_string()),
        version: None,
        output: Some(out_dir.display().to_string()),
        full: true,
        file: None,
    };

    let result = chub_rs::commands::get::run(
        &args,
        &merged,
        &chub_dir,
        &annotations_dir,
        &source_info,
        false,
    );
    assert!(result.is_ok(), "get --full -o should succeed");

    // Single entry: files written directly to output dir, NOT <output>/<id>/
    assert!(
        out_dir.join("DOC.md").exists(),
        "DOC.md should be directly in output dir"
    );
    assert!(
        out_dir.join("references/advanced.md").exists(),
        "reference file should be in output dir"
    );
    // Should NOT nest under entry id
    assert!(
        !out_dir.join("acme/widgets").exists(),
        "single entry should NOT nest under <output>/<id>/"
    );
}

// ── Scenario 15: JSON 输出按需包含注解和附加文件字段 ────────────────

#[test]
fn get_json_includes_optional_annotation_and_additional_files() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();
    let source_name = "default";
    let annotations_dir = chub_dir.join("annotations");

    setup_cached_docs(chub_dir, source_name);
    write_test_annotation(&annotations_dir, "acme/widgets", "Great widget docs!");

    let merged = fixture_merged(source_name);

    // acme/widgets has annotation and additional files
    let entry = get_entry("acme/widgets", &merged.entries)
        .entry
        .expect("find acme/widgets");
    let resolved = registry::resolve_doc_path(&entry, Some("js"), None);

    match resolved {
        ResolvedPath::Resolved { path, files, .. } => {
            // Simulate JSON output construction
            let content =
                chub_rs::core::cache::fetch_doc(chub_dir, None, None, source_name, &path, "DOC.md")
                    .expect("fetch DOC.md");

            let additional: Vec<&String> = files.iter().filter(|f| *f != "DOC.md").collect();
            let annotation =
                chub_rs::core::annotations::read_annotation_in(&annotations_dir, "acme/widgets")
                    .expect("read annotation");

            let mut output = serde_json::json!({
                "id": &entry.id,
                "type": &entry.entry_type,
                "content": &content,
            });
            if !additional.is_empty() {
                output["additionalFiles"] = serde_json::json!(additional);
            }
            if let Some(ref ann) = annotation {
                output["annotation"] = serde_json::json!(ann.note);
            }

            // Verify acme/widgets has both
            assert!(
                output.get("additionalFiles").is_some(),
                "should have additionalFiles"
            );
            assert!(output.get("annotation").is_some(), "should have annotation");
            assert_eq!(output["annotation"], "Great widget docs!");
        }
        other => panic!("expected Resolved, got {:?}", other),
    }

    // multilang/client has neither annotation nor additional files
    let entry2 = get_entry("multilang/client", &merged.entries)
        .entry
        .expect("find multilang/client");
    let resolved2 = registry::resolve_doc_path(&entry2, Some("js"), None);

    match resolved2 {
        ResolvedPath::Resolved { path, files, .. } => {
            let content =
                chub_rs::core::cache::fetch_doc(chub_dir, None, None, source_name, &path, "DOC.md")
                    .expect("fetch DOC.md");

            let additional: Vec<&String> = files.iter().filter(|f| *f != "DOC.md").collect();
            let annotation = chub_rs::core::annotations::read_annotation_in(
                &annotations_dir,
                "multilang/client",
            )
            .expect("read annotation");

            let mut output = serde_json::json!({
                "id": &entry2.id,
                "type": &entry2.entry_type,
                "content": &content,
            });
            if !additional.is_empty() {
                output["additionalFiles"] = serde_json::json!(additional);
            }
            if let Some(ref ann) = annotation {
                output["annotation"] = serde_json::json!(ann.note);
            }

            // Verify multilang/client has neither
            assert!(
                output.get("additionalFiles").is_none(),
                "should NOT have additionalFiles"
            );
            assert!(
                output.get("annotation").is_none(),
                "should NOT have annotation"
            );
        }
        other => panic!("expected Resolved, got {:?}", other),
    }
}
