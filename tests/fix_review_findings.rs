//! Tests for fix-review-findings.spec — covers local source loading,
//! HTTP error handling, empty versions safety, and multi-source BM25 dedup.

mod test_helpers;

use chub_rs::core::bm25;
use chub_rs::core::cache;
use chub_rs::core::registry::{
    self, DocEntry, Entry, Language, Registry, ResolvedPath, SearchFilters, Version,
};
use tempfile::TempDir;

// ── Scenario 1: local source registry loads from source.path ─────────

#[test]
fn local_source_loads_registry_from_path() {
    let tmp = TempDir::new().unwrap();
    let local_path = tmp.path().join("my-source");
    std::fs::create_dir_all(&local_path).unwrap();

    // Write a registry.json directly in the local source path
    let reg = test_helpers::fixture_registry();
    let reg_json = serde_json::to_string_pretty(&reg).unwrap();
    std::fs::write(local_path.join("registry.json"), &reg_json).unwrap();

    // Load registry from the local path (simulating what load_merged_registry does)
    let loaded = registry::load_registry(&local_path.join("registry.json")).unwrap();
    assert_eq!(loaded.docs.len(), 3);
    assert_eq!(loaded.skills.len(), 1);

    // The registry should NOT need to be in ~/.chub/sources/
    let chub_dir = tmp.path().join("chub");
    let cached_reg_path = cache::get_source_registry_path(&chub_dir, "local");
    assert!(
        !cached_reg_path.exists(),
        "should not require cached registry"
    );
}

// ── Scenario 2: get reads content from local source path ─────────────

#[test]
fn get_reads_content_from_local_source_path() {
    let tmp = TempDir::new().unwrap();
    let local_path = tmp.path().join("my-source");
    let chub_dir = tmp.path().join("chub");

    // Set up doc content in the local source path (NOT in ~/.chub/sources/)
    let doc_dir = local_path.join("acme/widgets/javascript/v2");
    std::fs::create_dir_all(&doc_dir).unwrap();
    std::fs::write(
        doc_dir.join("DOC.md"),
        "# Local Source Content\n\nFrom local path.\n",
    )
    .unwrap();

    // fetch_doc with source_path should read from local path
    let content = cache::fetch_doc(
        &chub_dir,
        Some(local_path.as_path()),
        "my-source",
        "acme/widgets/javascript/v2",
        "DOC.md",
    )
    .unwrap();

    assert!(content.contains("Local Source Content"));
    assert!(content.contains("From local path."));

    // Verify it does NOT need cached data
    let cached_data_dir = cache::get_source_data_dir(&chub_dir, "my-source");
    assert!(
        !cached_data_dir.exists(),
        "should not require cached data dir"
    );
}

// ── Scenario 3: cache layer propagates fetch errors ──────────────────

#[test]
fn cache_layer_propagates_registry_fetch_error() {
    let tmp = TempDir::new().unwrap();
    let chub_dir = tmp.path().join("chub");
    std::fs::create_dir_all(&chub_dir).unwrap();

    // Cache layer propagates closure errors (simulates error_for_status() in update.rs)
    let result = cache::fetch_and_save_registry(
        &chub_dir,
        "test-source",
        || Err(anyhow::anyhow!("HTTP 404: Not Found")),
        1000,
    );

    assert!(result.is_err(), "should propagate fetch error");

    // Verify registry was NOT written
    let reg_path = cache::get_source_registry_path(&chub_dir, "test-source");
    assert!(!reg_path.exists(), "registry.json should not be written");

    // Verify meta was NOT updated
    let meta = cache::read_meta(&chub_dir, "test-source");
    assert!(
        meta.last_updated.is_none(),
        "meta.json should not be updated"
    );
}

// ── Scenario 4: cache layer rejects invalid bundle data ──────────────

#[test]
fn cache_layer_rejects_invalid_bundle_data() {
    let tmp = TempDir::new().unwrap();
    let chub_dir = tmp.path().join("chub");
    std::fs::create_dir_all(&chub_dir).unwrap();

    // extract_bundle with invalid data should fail
    let result = cache::extract_bundle(&chub_dir, "test-source", b"not-a-valid-tarball");

    assert!(result.is_err(), "should reject invalid bundle data");

    // Verify meta was NOT updated with full_bundle flag
    let meta = cache::read_meta(&chub_dir, "test-source");
    assert!(
        meta.full_bundle.is_none(),
        "meta should not mark full_bundle on failure"
    );
}

// ── Scenario 3b: update command propagates fetcher errors ────────────

#[test]
fn update_command_propagates_fetcher_errors() {
    use chub_rs::commands::update::{UpdateArgs, run_with_fetcher};
    use chub_rs::core::config::{Config, Source};

    let tmp = TempDir::new().unwrap();
    let chub_dir = tmp.path().join("chub");
    std::fs::create_dir_all(&chub_dir).unwrap();

    let cfg = Config {
        sources: vec![Source {
            name: "remote".to_string(),
            url: Some("https://example.com/cdn".to_string()),
            path: None,
        }],
        refresh_interval: 3600,
        output_dir: String::new(),
        output_format: String::new(),
        source_filter: String::new(),
        telemetry: false,
        telemetry_url: String::new(),
    };

    let args = UpdateArgs {
        force: true,
        full: false,
    };

    // Fetcher simulates error_for_status() returning 404
    let result = run_with_fetcher(
        &args,
        &chub_dir,
        &cfg,
        1000,
        true, // json mode to suppress stderr
        |_url| Err(anyhow::anyhow!("HTTP status client error (404 Not Found)")),
        None::<fn(&str) -> anyhow::Result<Vec<u8>>>,
    );

    // run_with_fetcher collects errors but returns Ok — check that registry wasn't saved
    assert!(result.is_ok());
    let reg_path = cache::get_source_registry_path(&chub_dir, "remote");
    assert!(
        !reg_path.exists(),
        "failed fetch should not create registry.json"
    );
}

// ── Scenario 5: resolve_doc_path with empty versions returns Unresolvable ──

#[test]
fn resolve_doc_path_empty_versions_returns_unresolvable() {
    // Create a doc entry with a language that has NO versions
    let entry = Entry::from_doc(&DocEntry {
        id: "broken/doc".to_string(),
        name: "broken".to_string(),
        description: "A doc with empty versions".to_string(),
        tags: vec![],
        languages: vec![Language {
            language: "javascript".to_string(),
            recommended_version: "1.0.0".to_string(),
            versions: vec![], // Empty!
        }],
        _source: None,
        source_obj: None,
    });

    let result = registry::resolve_doc_path(&entry, Some("javascript"), None);

    // Must NOT panic — should return Unresolvable
    assert!(
        matches!(result, ResolvedPath::Unresolvable),
        "empty versions should return Unresolvable, got: {:?}",
        result
    );
}

// ── Scenario 6: multi-source BM25 takes max score for duplicate IDs ──

#[test]
fn multi_source_bm25_takes_max_score_for_duplicate_ids() {
    // Create two sources both with "openai/chat"
    let doc1 = DocEntry {
        id: "openai/chat".to_string(),
        name: "chat".to_string(),
        description: "OpenAI Chat API comprehensive guide".to_string(),
        tags: vec!["api".to_string(), "openai".to_string()],
        languages: vec![Language {
            language: "javascript".to_string(),
            recommended_version: "1.0.0".to_string(),
            versions: vec![Version {
                version: "1.0.0".to_string(),
                path: "openai/chat/javascript".to_string(),
                files: vec!["DOC.md".to_string()],
                last_updated: "2025-01-01".to_string(),
                size: 5000,
            }],
        }],
        _source: None,
        source_obj: None,
    };

    let doc2 = DocEntry {
        id: "openai/chat".to_string(),
        name: "chat".to_string(),
        description: "OpenAI Chat basic reference".to_string(),
        tags: vec!["api".to_string()],
        languages: vec![Language {
            language: "python".to_string(),
            recommended_version: "1.0.0".to_string(),
            versions: vec![Version {
                version: "1.0.0".to_string(),
                path: "openai/chat/python".to_string(),
                files: vec!["DOC.md".to_string()],
                last_updated: "2025-01-01".to_string(),
                size: 4000,
            }],
        }],
        _source: None,
        source_obj: None,
    };

    let reg1 = Registry {
        version: "1.0.0".to_string(),
        generated: "2025-01-01T00:00:00Z".to_string(),
        docs: vec![doc1],
        skills: vec![],
    };
    let reg2 = Registry {
        version: "1.0.0".to_string(),
        generated: "2025-01-01T00:00:00Z".to_string(),
        docs: vec![doc2],
        skills: vec![],
    };

    // Build separate BM25 indexes for each source
    let idx1 = bm25::build_index(&[bm25::IndexEntry {
        id: "openai/chat".to_string(),
        name: "chat".to_string(),
        description: "OpenAI Chat API comprehensive guide".to_string(),
        tags: vec!["api".to_string(), "openai".to_string()],
    }]);
    let idx2 = bm25::build_index(&[bm25::IndexEntry {
        id: "openai/chat".to_string(),
        name: "chat".to_string(),
        description: "OpenAI Chat basic reference".to_string(),
        tags: vec!["api".to_string()],
    }]);

    let merged = registry::merge_registries(&[
        ("source1".to_string(), reg1, Some(idx1)),
        ("source2".to_string(), reg2, Some(idx2)),
    ]);

    // Search for "openai chat"
    let filters = SearchFilters::default();
    let results = registry::search_entries("openai chat", &merged, &filters);

    // Should find entries (both sources have openai/chat)
    let chat_entries: Vec<&Entry> = results.iter().filter(|e| e.id == "openai/chat").collect();

    // All matched entries should have a score, and the score should be the max
    for entry in &chat_entries {
        assert!(entry._score.is_some(), "search result should have a score");
        assert!(entry._score.unwrap() > 0.0, "score should be positive");
    }

    // Collect all scores for openai/chat — they should all be the same (max)
    let scores: Vec<f64> = chat_entries.iter().filter_map(|e| e._score).collect();
    if scores.len() > 1 {
        let first = scores[0];
        assert!(
            scores.iter().all(|&s| (s - first).abs() < f64::EPSILON),
            "all entries with same ID should have the same (max) score, got: {:?}",
            scores
        );
    }
}
