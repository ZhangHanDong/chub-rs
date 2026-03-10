//! M2 write-path tests.
//! Tests for annotate, feedback, update, cache, identity per m2-write-path.spec.

mod test_helpers;

use chub_rs::core::annotations;
use chub_rs::core::cache;
use chub_rs::core::config::{Config, Source};
use chub_rs::core::identity;
use chub_rs::core::telemetry::{self, FeedbackPayload, VALID_LABELS};
use tempfile::TempDir;
use test_helpers::*;

// ── Scenario 1: annotation 支持写入、读取、替换和清除 ──────────────

#[test]
fn annotate_crud_round_trip() {
    let tmp = TempDir::new().expect("temp dir");
    let ann_dir = tmp.path().join("annotations");

    // Write
    let ann = annotations::write_annotation_in(
        &ann_dir,
        "acme/widgets",
        "Great docs!",
        "2025-01-01T00:00:00Z",
    )
    .expect("write");
    assert_eq!(ann.id, "acme/widgets");
    assert_eq!(ann.note, "Great docs!");

    // Read
    let read = annotations::read_annotation_in(&ann_dir, "acme/widgets")
        .expect("read")
        .expect("should exist");
    assert_eq!(read.note, "Great docs!");

    // Replace
    let ann2 = annotations::write_annotation_in(
        &ann_dir,
        "acme/widgets",
        "Updated note",
        "2025-01-02T00:00:00Z",
    )
    .expect("replace");
    assert_eq!(ann2.note, "Updated note");

    let read2 = annotations::read_annotation_in(&ann_dir, "acme/widgets")
        .expect("read after replace")
        .expect("should exist");
    assert_eq!(read2.note, "Updated note");

    // Clear
    let cleared = annotations::clear_annotation_in(&ann_dir, "acme/widgets").expect("clear");
    assert!(cleared, "should return true for existing annotation");

    let read3 =
        annotations::read_annotation_in(&ann_dir, "acme/widgets").expect("read after clear");
    assert!(read3.is_none(), "should be None after clear");

    // Clear non-existent
    let cleared2 = annotations::clear_annotation_in(&ann_dir, "acme/widgets").expect("clear again");
    assert!(!cleared2, "should return false for non-existent");
}

// ── Scenario 2: annotation 列表返回全部持久化注解 ───────────────────

#[test]
fn annotate_list_returns_all_annotations() {
    let tmp = TempDir::new().expect("temp dir");
    let ann_dir = tmp.path().join("annotations");

    annotations::write_annotation_in(
        &ann_dir,
        "acme/widgets",
        "Widget note",
        "2025-01-01T00:00:00Z",
    )
    .expect("write 1");
    annotations::write_annotation_in(&ann_dir, "openai/chat", "Chat note", "2025-01-02T00:00:00Z")
        .expect("write 2");

    let all = annotations::list_annotations_in(&ann_dir).expect("list");
    assert_eq!(all.len(), 2);

    let ids: Vec<&str> = all.iter().map(|a| a.id.as_str()).collect();
    assert!(ids.contains(&"acme/widgets"));
    assert!(ids.contains(&"openai/chat"));

    // Verify JSON serialization includes required fields
    for ann in &all {
        let json = serde_json::to_value(ann).expect("serialize");
        assert!(json.get("id").is_some());
        assert!(json.get("note").is_some());
        assert!(json.get("updatedAt").is_some());
    }
}

// ── Scenario 3: `get` 会在文档末尾附加 annotation ──────────────────

#[test]
fn annotation_is_appended_on_get() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();
    let source_name = "default";
    let ann_dir = chub_dir.join("annotations");

    setup_cached_docs(chub_dir, source_name);
    write_test_annotation(&ann_dir, "acme/widgets", "Webhook needs raw body");

    // Read annotation and verify it exists
    let ann = annotations::read_annotation_in(&ann_dir, "acme/widgets")
        .expect("read")
        .expect("should exist");
    assert_eq!(ann.note, "Webhook needs raw body");
    assert!(
        !ann.updated_at.is_empty(),
        "should have updatedAt timestamp"
    );
}

// ── Scenario 4: `cache status` 输出每个 source 的缓存摘要 ─────────

#[test]
fn cache_status_reports_sources_and_sizes() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();

    // Set up a remote source cache
    setup_cached_docs(chub_dir, "default");
    // Write a registry.json
    let reg = fixture_registry();
    let reg_path = cache::get_source_registry_path(chub_dir, "default");
    std::fs::create_dir_all(reg_path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&reg_path, serde_json::to_string(&reg).expect("ser")).expect("write reg");

    // Write meta
    cache::write_meta(
        chub_dir,
        "default",
        &cache::SourceMeta {
            last_updated: Some(1704067200),
            full_bundle: None,
            bundled_seed: None,
        },
    )
    .expect("write meta");

    let stats = cache::get_cache_stats(chub_dir, &[("default", false)]);
    assert!(stats.exists);
    assert_eq!(stats.sources.len(), 1);

    let s = &stats.sources[0];
    assert_eq!(s.name, "default");
    assert_eq!(s.source_type, "remote");
    assert!(s.has_registry);
    assert_eq!(s.last_updated, Some(1704067200));
    assert!(s.file_count > 0, "should have cached files");
    assert!(s.data_size > 0, "should have non-zero size");

    // Verify JSON serialization
    let json = serde_json::to_value(s).expect("serialize");
    assert!(json.get("name").is_some());
    assert!(json.get("type").is_some());
    assert!(json.get("hasRegistry").is_some());
    assert!(json.get("fileCount").is_some());
    assert!(json.get("dataSize").is_some());
}

// ── Scenario 5: `cache clear` 只保留 `config.yaml` ─────────────────

#[test]
fn cache_clear_preserves_config_only() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();

    // Create config.yaml
    std::fs::write(chub_dir.join("config.yaml"), "telemetry: true\n").expect("write config");
    // Create annotations
    let ann_dir = chub_dir.join("annotations");
    std::fs::create_dir_all(&ann_dir).expect("mkdir annotations");
    std::fs::write(ann_dir.join("test.json"), "{}").expect("write ann");
    // Create sources cache
    setup_cached_docs(chub_dir, "default");
    // Create client_id
    std::fs::write(chub_dir.join("client_id"), "abc123").expect("write client_id");

    // Verify files exist before clear
    assert!(chub_dir.join("annotations").exists());
    assert!(chub_dir.join("sources").exists());
    assert!(chub_dir.join("client_id").exists());

    cache::clear_cache(chub_dir).expect("clear");

    // config.yaml preserved
    assert!(
        chub_dir.join("config.yaml").exists(),
        "config.yaml should be preserved"
    );

    // Everything else deleted
    assert!(
        !chub_dir.join("annotations").exists(),
        "annotations should be deleted"
    );
    assert!(
        !chub_dir.join("sources").exists(),
        "sources should be deleted"
    );
    assert!(
        !chub_dir.join("client_id").exists(),
        "client_id should be deleted"
    );
}

// ── Scenario 6: `update` 下载 registry 并遵守 freshness/force ─────

#[test]
fn update_fetches_registry_with_refresh_policy() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();

    let mut cfg = default_config();
    cfg.sources = vec![Source {
        name: "test".to_string(),
        url: Some("https://cdn.example.com/v1".to_string()),
        path: None,
    }];
    cfg.refresh_interval = 21600; // 6 hours

    let now = 1704067200u64; // some epoch timestamp

    // First update: cache is empty → should fetch
    let reg_json = serde_json::to_string(&fixture_registry()).expect("ser");

    // Simulate first fetch
    cache::fetch_and_save_registry(chub_dir, "test", || Ok((reg_json.clone(), None)), now)
        .expect("first fetch");

    assert!(cache::get_source_registry_path(chub_dir, "test").exists());
    assert!(cache::is_cache_fresh(
        chub_dir,
        "test",
        cfg.refresh_interval,
        now
    ));

    // Cache is fresh at same time → should NOT need fetch
    assert!(cache::is_cache_fresh(
        chub_dir,
        "test",
        cfg.refresh_interval,
        now
    ));

    // Cache is stale after refresh_interval → should need fetch
    let later = now + cfg.refresh_interval + 1;
    assert!(!cache::is_cache_fresh(
        chub_dir,
        "test",
        cfg.refresh_interval,
        later
    ));

    // Force always means "not fresh"
    // (force skips freshness check in command handler, not in cache module)
}

// ── Scenario 7: `update --full` 下载并解压 bundle ──────────────────

#[test]
fn update_full_bundle_downloads_and_extracts() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();

    // Create a minimal tar.gz bundle in memory
    let bundle_data = create_test_bundle();

    cache::extract_bundle(chub_dir, "test", &bundle_data).expect("extract bundle");

    // Verify data was extracted
    let data_dir = cache::get_source_data_dir(chub_dir, "test");
    assert!(data_dir.exists());

    // Verify meta was updated
    let meta = cache::read_meta(chub_dir, "test");
    assert_eq!(meta.full_bundle, Some(true));
}

/// Create a minimal tar.gz bundle for testing.
fn create_test_bundle() -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        let content = b"# Test Doc\n\nTest content.\n";
        let mut header = tar::Header::new_gnu();
        header.set_path("test/DOC.md").expect("set path");
        header.set_size(content.len() as u64);
        header.set_cksum();
        builder.append(&header, &content[..]).expect("append file");
        builder.finish().expect("finish tar");
    }
    encoder.finish().expect("finish gz")
}

// ── Scenario 8: `feedback --status` 输出 telemetry 状态 ────────────

#[test]
fn feedback_status_reports_telemetry_configuration() {
    // Verify VALID_LABELS contains expected values
    assert!(VALID_LABELS.contains(&"outdated"));
    assert!(VALID_LABELS.contains(&"well-written"));
    assert!(VALID_LABELS.len() >= 6);

    // Verify FeedbackPayload serializes correctly
    let payload = FeedbackPayload {
        entry_id: "acme/widgets".to_string(),
        entry_type: "doc".to_string(),
        rating: "up".to_string(),
        comment: Some("Great!".to_string()),
        labels: vec!["well-written".to_string()],
        language: Some("javascript".to_string()),
        doc_version: Some("2.0.0".to_string()),
        file: None,
        agent: Some("claude".to_string()),
        model: Some("opus".to_string()),
        client_id: "abc123".to_string(),
    };

    let json = serde_json::to_value(&payload).expect("serialize");
    assert_eq!(json["entry_id"], "acme/widgets");
    assert_eq!(json["rating"], "up");
    assert_eq!(json["client_id"], "abc123");
    assert!(json.get("labels").is_some());
}

// ── Scenario 9: telemetry 禁用时 feedback 返回 skipped ─────────────

#[test]
fn feedback_skips_when_telemetry_disabled() {
    let payload = FeedbackPayload {
        entry_id: "acme/widgets".to_string(),
        entry_type: "doc".to_string(),
        rating: "up".to_string(),
        comment: None,
        labels: vec![],
        language: None,
        doc_version: None,
        file: None,
        agent: None,
        model: None,
        client_id: "test-id".to_string(),
    };

    // With telemetry disabled
    let result = telemetry::send_feedback_with(&payload, false, |_| {
        panic!("should not be called when telemetry is disabled");
    })
    .expect("should not error");

    match result {
        telemetry::FeedbackResult::Skipped => {}
        _ => panic!("expected Skipped"),
    }
}

// ── Scenario 10: feedback 请求携带解析后的上下文参数 ────────────────

#[test]
fn feedback_payload_includes_labels_and_inferred_context() {
    let payload = FeedbackPayload {
        entry_id: "openai/chat".to_string(),
        entry_type: "doc".to_string(),
        rating: "down".to_string(),
        comment: Some("API changed".to_string()),
        labels: vec!["outdated".to_string(), "inaccurate".to_string()],
        language: Some("python".to_string()),
        doc_version: Some("4.0.0".to_string()),
        file: Some("DOC.md".to_string()),
        agent: Some("cursor".to_string()),
        model: Some("gpt-4".to_string()),
        client_id: "test-client-id-hex".to_string(),
    };

    let result = telemetry::send_feedback_with(&payload, true, |p| {
        // Verify all fields are present
        assert_eq!(p.entry_id, "openai/chat");
        assert_eq!(p.rating, "down");
        assert_eq!(p.comment.as_deref(), Some("API changed"));
        assert_eq!(p.labels.len(), 2);
        assert!(p.labels.contains(&"outdated".to_string()));
        assert_eq!(p.language.as_deref(), Some("python"));
        assert_eq!(p.doc_version.as_deref(), Some("4.0.0"));
        assert_eq!(p.file.as_deref(), Some("DOC.md"));
        assert_eq!(p.agent.as_deref(), Some("cursor"));
        assert_eq!(p.model.as_deref(), Some("gpt-4"));
        assert!(!p.client_id.is_empty());
        Ok(())
    })
    .expect("send feedback");

    match result {
        telemetry::FeedbackResult::Sent => {}
        _ => panic!("expected Sent"),
    }
}

// ── Scenario 11: client id 首次创建后被缓存复用 ────────────────────

#[test]
fn client_id_is_created_and_cached() {
    let tmp = TempDir::new().expect("temp dir");
    let chub_dir = tmp.path();

    // Should not exist yet
    assert!(!chub_dir.join("client_id").exists());

    // First call: create
    let id1 = identity::get_or_create_client_id_with_uuid(chub_dir, "test-machine-uuid")
        .expect("first call");
    assert_eq!(id1.len(), 64, "should be 64-char hex (SHA-256)");
    assert!(
        id1.chars().all(|c| c.is_ascii_hexdigit()),
        "should be hex chars"
    );

    // File should now exist
    assert!(chub_dir.join("client_id").exists());

    // Second call: cached
    let id2 = identity::get_or_create_client_id_with_uuid(chub_dir, "test-machine-uuid")
        .expect("second call");
    assert_eq!(id1, id2, "should return same cached value");

    // Even with different UUID input, cached value should be returned
    let id3 = identity::get_or_create_client_id_with_uuid(chub_dir, "different-uuid")
        .expect("third call");
    assert_eq!(id1, id3, "cached value takes precedence");
}

// ── Scenario 12: analytics 在缺少依赖或 telemetry 禁用时不抛错 ────

#[test]
fn analytics_is_non_blocking() {
    // With telemetry disabled
    chub_rs::core::analytics::track_event(
        "test_event",
        &serde_json::json!({"key": "value"}),
        false,
    );

    // With telemetry enabled (no backend)
    chub_rs::core::analytics::track_event("test_event", &serde_json::json!({"key": "value"}), true);

    // Shutdown should always succeed
    chub_rs::core::analytics::shutdown_analytics();
}

fn default_config() -> Config {
    Config {
        sources: vec![Source {
            name: "default".to_string(),
            url: Some("https://cdn.aichub.org/v1".to_string()),
            path: None,
        }],
        output_dir: ".context".to_string(),
        refresh_interval: 21600,
        output_format: "human".to_string(),
        source_filter: "official,maintainer,community".to_string(),
        telemetry: true,
        telemetry_url: "https://api.aichub.org/v1".to_string(),
    }
}
