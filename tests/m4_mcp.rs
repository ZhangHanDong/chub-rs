use chub_rs::core::bm25;
use chub_rs::core::registry::{DocEntry, Language, Registry, SkillEntry, Version};
use chub_rs::mcp::McpContext;
use chub_rs::mcp::protocol::JsonRpcRequest;
use std::collections::HashMap;
use tempfile::TempDir;

// ── Fixture helpers ─────────────────────────────────────────────────

fn fixture_registry() -> Registry {
    Registry {
        version: "1.0.0".to_string(),
        generated: "2025-01-01T00:00:00Z".to_string(),
        docs: vec![DocEntry {
            id: "acme/widgets".to_string(),
            name: "widgets".to_string(),
            description: "Acme widget API documentation".to_string(),
            tags: vec!["api".to_string(), "widgets".to_string()],
            languages: vec![
                Language {
                    language: "javascript".to_string(),
                    recommended_version: "2.0.0".to_string(),
                    versions: vec![Version {
                        version: "2.0.0".to_string(),
                        path: "acme/widgets/javascript/v2".to_string(),
                        files: vec!["DOC.md".to_string(), "references/advanced.md".to_string()],
                        last_updated: "2025-01-01".to_string(),
                        size: 12345,
                    }],
                },
                Language {
                    language: "python".to_string(),
                    recommended_version: "1.0.0".to_string(),
                    versions: vec![Version {
                        version: "1.0.0".to_string(),
                        path: "acme/widgets/python".to_string(),
                        files: vec!["DOC.md".to_string()],
                        last_updated: "2025-01-01".to_string(),
                        size: 5000,
                    }],
                },
            ],
            _source: None,
            source_obj: None,
        }],
        skills: vec![SkillEntry {
            id: "testskills/deploy".to_string(),
            name: "deploy".to_string(),
            description: "Deployment automation skill".to_string(),
            tags: vec!["automation".to_string(), "deploy".to_string()],
            path: "testskills/deploy".to_string(),
            files: vec!["SKILL.md".to_string()],
            last_updated: "2025-01-01".to_string(),
            size: 3000,
            _source: None,
            source_obj: None,
        }],
    }
}

fn test_context(tmp: &TempDir) -> McpContext {
    let source_name = "test-source";
    let reg = fixture_registry();

    // Build search index
    let index_entries: Vec<bm25::IndexEntry> = reg
        .docs
        .iter()
        .map(|d| bm25::IndexEntry {
            id: d.id.clone(),
            name: d.name.clone(),
            description: d.description.clone(),
            tags: d.tags.clone(),
        })
        .chain(reg.skills.iter().map(|s| bm25::IndexEntry {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            tags: s.tags.clone(),
        }))
        .collect();
    let idx = bm25::build_index(&index_entries);

    let merged =
        chub_rs::core::registry::merge_registries(&[(source_name.to_string(), reg, Some(idx))]);

    let chub_dir = tmp.path().join("chub");
    let annotations_dir = chub_dir.join("annotations");
    std::fs::create_dir_all(&annotations_dir).unwrap();

    // Set up cached doc files
    setup_cached_docs(&chub_dir, source_name);

    McpContext {
        entries: merged.entries,
        search_index: merged.search_index,
        chub_dir,
        annotations_dir,
        source_paths: HashMap::from([(source_name.to_string(), None)]),
        telemetry_enabled: false,
        feedback_endpoint: "https://example.com/api".to_string(),
        version: "0.1.1".to_string(),
    }
}

fn setup_cached_docs(chub_dir: &std::path::Path, source_name: &str) {
    // acme/widgets javascript
    let widget_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("acme/widgets/javascript/v2");
    std::fs::create_dir_all(&widget_dir).unwrap();
    std::fs::create_dir_all(widget_dir.join("references")).unwrap();
    std::fs::write(
        widget_dir.join("DOC.md"),
        "# Acme Widgets\n\nWidget docs.\n",
    )
    .unwrap();
    std::fs::write(
        widget_dir.join("references/advanced.md"),
        "# Advanced Widgets\n",
    )
    .unwrap();

    // acme/widgets python
    let py_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("acme/widgets/python");
    std::fs::create_dir_all(&py_dir).unwrap();
    std::fs::write(py_dir.join("DOC.md"), "# Acme Widgets (Python)\n").unwrap();

    // testskills/deploy
    let skill_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("testskills/deploy");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "# Deploy Skill\n\nDeploy steps.\n",
    )
    .unwrap();
}

fn dispatch_request(
    ctx: &McpContext,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        method: method.to_string(),
        params: Some(params),
    };
    let resp = ctx.dispatch(&req).unwrap();
    resp.result.unwrap_or_else(
        || serde_json::json!({ "error": resp.error.map(|e| e.message).unwrap_or_default() }),
    )
}

fn parse_text_content(result: &serde_json::Value) -> serde_json::Value {
    let text = result["content"][0]["text"].as_str().unwrap_or("{}");
    serde_json::from_str(text).unwrap_or(serde_json::json!(text))
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn mcp_server_initialize() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp);

    let result = dispatch_request(
        &ctx,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" }
        }),
    );

    assert_eq!(result["serverInfo"]["name"], "chub");
    assert_eq!(result["serverInfo"]["version"], "0.1.1");
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert!(result.get("capabilities").is_some());
    assert!(result["capabilities"].get("tools").is_some());
    assert!(result["capabilities"].get("resources").is_some());
}

#[test]
fn mcp_tool_search_returns_simplified_results() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp);

    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_search",
            "arguments": { "query": "widget", "limit": 5 }
        }),
    );

    let data = parse_text_content(&result);
    assert!(data.get("results").is_some());
    assert!(data.get("total").is_some());
    assert!(data.get("showing").is_some());

    let results = data["results"].as_array().unwrap();
    assert!(!results.is_empty());
    // Check simplified fields
    let first = &results[0];
    assert!(first.get("id").is_some());
    assert!(first.get("name").is_some());
    assert!(first.get("type").is_some());
    assert!(first.get("description").is_some());
    assert!(first.get("tags").is_some());
    // Internal fields should NOT be present
    assert!(first.get("_source").is_none());
    assert!(first.get("_score").is_none());
}

#[test]
fn mcp_tool_get_text_and_lang_errors() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp);

    // 1. Fetch skill (no lang needed)
    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_get",
            "arguments": { "id": "testskills/deploy" }
        }),
    );
    assert!(
        result.get("isError").is_none(),
        "Skill fetch should not be an error"
    );
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Deploy Skill"));

    // 2. Fetch doc with lang
    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_get",
            "arguments": { "id": "acme/widgets", "lang": "javascript" }
        }),
    );
    assert!(result.get("isError").is_none());
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Acme Widgets"));

    // 3. Fetch doc without lang (should error with available languages)
    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_get",
            "arguments": { "id": "acme/widgets" }
        }),
    );
    assert_eq!(result["isError"], true);
    let error_data = parse_text_content(&result);
    assert!(
        error_data["error"]
            .as_str()
            .unwrap()
            .contains("languages available")
    );
    assert!(error_data.get("available").is_some());
}

#[test]
fn mcp_tool_get_blocks_path_traversal() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp);

    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_get",
            "arguments": { "id": "acme/widgets", "lang": "javascript", "file": "../../../etc/passwd" }
        }),
    );

    assert_eq!(result["isError"], true);
    let error_data = parse_text_content(&result);
    assert!(
        error_data["error"]
            .as_str()
            .unwrap()
            .contains("Path traversal"),
        "Should mention path traversal"
    );
}

#[test]
fn mcp_tool_list_returns_entries() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp);

    // List all
    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_list",
            "arguments": {}
        }),
    );
    let data = parse_text_content(&result);
    assert!(data.get("entries").is_some());
    assert!(data.get("total").is_some());
    assert!(data.get("showing").is_some());
    assert_eq!(data["total"], 2); // 1 doc + 1 skill

    // List with limit
    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_list",
            "arguments": { "limit": 1 }
        }),
    );
    let data = parse_text_content(&result);
    assert_eq!(data["showing"], 1);
    assert_eq!(data["total"], 2);

    // List with lang filter
    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_list",
            "arguments": { "lang": "python" }
        }),
    );
    let data = parse_text_content(&result);
    // Should only include docs with python, not skills
    assert_eq!(data["total"], 1);
}

#[test]
fn mcp_tool_annotate_modes() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp);
    let now = "2025-01-01T00:00:00Z";

    // 1. List (empty)
    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_annotate",
            "arguments": { "list": true }
        }),
    );
    let data = parse_text_content(&result);
    assert_eq!(data["total"], 0);
    assert!(data.get("annotations").is_some());

    // 2. Write
    let result = chub_rs::mcp::handlers::handle_annotate(
        &ctx,
        &serde_json::json!({ "id": "acme/widgets", "note": "Test annotation" }),
        now,
    );
    let data = parse_text_content(&result);
    assert_eq!(data["status"], "saved");
    assert!(data.get("annotation").is_some());

    // 3. Read
    let result = chub_rs::mcp::handlers::handle_annotate(
        &ctx,
        &serde_json::json!({ "id": "acme/widgets" }),
        now,
    );
    let data = parse_text_content(&result);
    assert!(data.get("annotation").is_some());
    assert_eq!(data["annotation"]["note"], "Test annotation");

    // 4. List (should have 1)
    let result =
        chub_rs::mcp::handlers::handle_annotate(&ctx, &serde_json::json!({ "list": true }), now);
    let data = parse_text_content(&result);
    assert_eq!(data["total"], 1);

    // 5. Clear
    let result = chub_rs::mcp::handlers::handle_annotate(
        &ctx,
        &serde_json::json!({ "id": "acme/widgets", "clear": true }),
        now,
    );
    let data = parse_text_content(&result);
    assert_eq!(data["status"], "cleared");

    // 6. Read after clear
    let result = chub_rs::mcp::handlers::handle_annotate(
        &ctx,
        &serde_json::json!({ "id": "acme/widgets" }),
        now,
    );
    let data = parse_text_content(&result);
    assert_eq!(data["status"], "no_annotation");
}

#[test]
fn mcp_tool_annotate_validates_entry_id() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp);
    let now = "2025-01-01T00:00:00Z";

    // ID too long (201 chars)
    let long_id = "a".repeat(201);
    let result = chub_rs::mcp::handlers::handle_annotate(
        &ctx,
        &serde_json::json!({ "id": long_id, "note": "test" }),
        now,
    );
    assert_eq!(result["isError"], true);
    let data = parse_text_content(&result);
    assert!(data["error"].as_str().unwrap().contains("too long"));

    // Invalid characters
    let result = chub_rs::mcp::handlers::handle_annotate(
        &ctx,
        &serde_json::json!({ "id": "bad<script>id", "note": "test" }),
        now,
    );
    assert_eq!(result["isError"], true);
    let data = parse_text_content(&result);
    assert!(
        data["error"]
            .as_str()
            .unwrap()
            .contains("invalid characters")
    );
}

#[test]
fn mcp_tool_feedback_skips_when_telemetry_disabled() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp); // telemetry_enabled = false

    let result = dispatch_request(
        &ctx,
        "tools/call",
        serde_json::json!({
            "name": "chub_feedback",
            "arguments": { "id": "acme/widgets", "rating": "up" }
        }),
    );

    let data = parse_text_content(&result);
    assert_eq!(data["status"], "skipped");
    assert_eq!(data["reason"], "telemetry_disabled");
}

#[test]
fn mcp_resource_registry_returns_simplified_entries() {
    let tmp = TempDir::new().unwrap();
    let ctx = test_context(&tmp);

    let result = dispatch_request(
        &ctx,
        "resources/read",
        serde_json::json!({ "uri": "chub://registry" }),
    );

    assert!(result.get("contents").is_some());
    let contents = result["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0]["uri"], "chub://registry");
    assert_eq!(contents[0]["mimeType"], "application/json");

    let registry_text = contents[0]["text"].as_str().unwrap();
    let registry: serde_json::Value = serde_json::from_str(registry_text).unwrap();

    assert!(registry.get("entries").is_some());
    assert!(registry.get("total").is_some());
    assert_eq!(registry["total"], 2); // 1 doc + 1 skill

    let entries = registry["entries"].as_array().unwrap();
    for entry in entries {
        assert!(entry.get("id").is_some());
        assert!(entry.get("name").is_some());
        assert!(entry.get("type").is_some());
        assert!(entry.get("description").is_some());
        assert!(entry.get("tags").is_some());
        // Internal fields should NOT be present
        assert!(entry.get("_source").is_none());
        assert!(entry.get("_score").is_none());
    }

    // Doc entry should have languages with version details
    let doc_entry = entries.iter().find(|e| e["type"] == "doc").unwrap();
    assert!(doc_entry.get("languages").is_some());
    let langs = doc_entry["languages"].as_array().unwrap();
    assert!(!langs.is_empty());
    assert!(langs[0].get("language").is_some());
    assert!(langs[0].get("versions").is_some());
    assert!(langs[0].get("recommended").is_some());
}
