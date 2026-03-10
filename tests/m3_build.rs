use chub_rs::core::build;
use chub_rs::core::frontmatter;
use std::path::Path;
use tempfile::TempDir;

// ── Fixture helpers ─────────────────────────────────────────────────

fn write_doc_md(dir: &Path, author: &str, doc_name: &str, lang: &str, content: &str) {
    let doc_dir = dir.join(author).join("docs").join(doc_name).join(lang);
    std::fs::create_dir_all(&doc_dir).unwrap();
    std::fs::write(doc_dir.join("DOC.md"), content).unwrap();
}

fn write_skill_md(dir: &Path, author: &str, skill_name: &str, content: &str) {
    let skill_dir = dir.join(author).join("skills").join(skill_name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
}

fn minimal_doc_frontmatter(name: &str, lang: &str, version: &str) -> String {
    format!(
        r#"---
name: {name}
description: "A test doc for {name}"
metadata:
  languages: "{lang}"
  versions: "{version}"
  source: "maintainer"
  tags: "test, api"
  updated-on: "2025-01-01"
---
# {name} Documentation
Some content here.
"#
    )
}

fn minimal_skill_frontmatter(name: &str) -> String {
    format!(
        r#"---
name: {name}
description: "A test skill for {name}"
metadata:
  source: "community"
  tags: "skill, automation"
  updated-on: "2025-01-01"
---
# {name} Skill
Skill instructions here.
"#
    )
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn build_minimal_content_dir() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");
    let dist_dir = tmp.path().join("dist");

    write_doc_md(
        &content_dir,
        "acme",
        "widgets",
        "javascript",
        &minimal_doc_frontmatter("widgets", "javascript", "2.0.0"),
    );

    let result = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap();

    // Write output
    build::write_build_output(&result, &content_dir, &dist_dir).unwrap();

    // Verify registry.json exists and has correct structure
    let reg_path = dist_dir.join("registry.json");
    assert!(reg_path.exists(), "registry.json should exist");

    let reg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&reg_path).unwrap()).unwrap();
    assert_eq!(reg["version"], "1.0.0");
    assert_eq!(reg["docs"].as_array().unwrap().len(), 1);
    assert_eq!(reg["docs"][0]["id"], "acme/widgets");

    // Verify search-index.json exists
    let idx_path = dist_dir.join("search-index.json");
    assert!(idx_path.exists(), "search-index.json should exist");

    // Verify content was copied
    let copied_doc = dist_dir
        .join("acme")
        .join("docs")
        .join("widgets")
        .join("javascript")
        .join("DOC.md");
    assert!(copied_doc.exists(), "DOC.md should be copied to output");
}

#[test]
fn frontmatter_parses_required_doc_fields() {
    let content = r#"---
name: widget-api
description: "Widget management API"
metadata:
  languages: "javascript, python"
  versions: "1.0.0, 2.0.0"
  source: "maintainer"
  tags: "api, widgets"
  updated-on: "2025-03-10"
---
# Widget API
Documentation content.
"#;
    let fm = frontmatter::parse_frontmatter(content);

    assert_eq!(
        fm.attributes.get("name").unwrap().as_str().unwrap(),
        "widget-api"
    );
    assert_eq!(
        fm.attributes.get("description").unwrap().as_str().unwrap(),
        "Widget management API"
    );

    let meta = fm.attributes.get("metadata").unwrap();
    assert_eq!(
        meta.get("languages").unwrap().as_str().unwrap(),
        "javascript, python"
    );
    assert_eq!(
        meta.get("versions").unwrap().as_str().unwrap(),
        "1.0.0, 2.0.0"
    );
    assert_eq!(meta.get("source").unwrap().as_str().unwrap(), "maintainer");
    assert_eq!(meta.get("tags").unwrap().as_str().unwrap(), "api, widgets");
    assert_eq!(
        meta.get("updated-on").unwrap().as_str().unwrap(),
        "2025-03-10"
    );
    assert!(fm.body.contains("# Widget API"));
}

#[test]
fn build_groups_multi_author_and_language_entries() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");

    // Author "acme" with a JS doc
    write_doc_md(
        &content_dir,
        "acme",
        "sdk",
        "javascript",
        &minimal_doc_frontmatter("sdk", "javascript", "1.0.0"),
    );

    // Author "multilang" with python and javascript variants of same doc
    write_doc_md(
        &content_dir,
        "multilang",
        "client",
        "python",
        &minimal_doc_frontmatter("client", "python", "1.0.0"),
    );
    write_doc_md(
        &content_dir,
        "multilang",
        "client",
        "javascript",
        &minimal_doc_frontmatter("client", "javascript", "1.0.0"),
    );

    let result = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap();

    // Should have 2 doc entries: acme/sdk and multilang/client
    assert_eq!(result.registry.docs.len(), 2);

    let acme_sdk = result
        .registry
        .docs
        .iter()
        .find(|d| d.id == "acme/sdk")
        .unwrap();
    assert_eq!(acme_sdk.languages.len(), 1);
    assert_eq!(acme_sdk.languages[0].language, "javascript");

    let ml_client = result
        .registry
        .docs
        .iter()
        .find(|d| d.id == "multilang/client")
        .unwrap();
    // Should have 2 languages aggregated under same ID
    assert_eq!(ml_client.languages.len(), 2);
    let lang_names: Vec<&str> = ml_client
        .languages
        .iter()
        .map(|l| l.language.as_str())
        .collect();
    assert!(lang_names.contains(&"javascript"));
    assert!(lang_names.contains(&"python"));
}

#[test]
fn build_discovers_skills() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");

    write_skill_md(
        &content_dir,
        "testskills",
        "deploy",
        &minimal_skill_frontmatter("deploy"),
    );

    let result = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap();

    assert_eq!(result.registry.skills.len(), 1);
    let skill = &result.registry.skills[0];
    assert_eq!(skill.id, "testskills/deploy");
    assert_eq!(skill.name, "deploy");
    assert!(skill.files.contains(&"SKILL.md".to_string()));
}

#[test]
fn build_uses_author_registry_when_present() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");
    let author_dir = content_dir.join("custom");
    std::fs::create_dir_all(&author_dir).unwrap();

    // Write a custom registry.json in the author directory
    let author_reg = serde_json::json!({
        "docs": [{
            "name": "custom-api",
            "description": "Custom API doc",
            "source": "maintainer",
            "tags": ["custom"],
            "languages": [{
                "language": "python",
                "recommendedVersion": "1.0.0",
                "versions": [{
                    "version": "1.0.0",
                    "path": "docs/custom-api/python",
                    "files": ["DOC.md"],
                    "size": 100,
                    "lastUpdated": "2025-01-01"
                }]
            }]
        }],
        "skills": []
    });
    std::fs::write(
        author_dir.join("registry.json"),
        serde_json::to_string_pretty(&author_reg).unwrap(),
    )
    .unwrap();

    // Also create the content files so copy works
    let doc_dir = author_dir.join("docs").join("custom-api").join("python");
    std::fs::create_dir_all(&doc_dir).unwrap();
    std::fs::write(doc_dir.join("DOC.md"), "# Custom API\nContent.").unwrap();

    let result = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap();

    assert_eq!(result.registry.docs.len(), 1);
    let doc = &result.registry.docs[0];
    // ID should be prefixed with author name
    assert_eq!(doc.id, "custom/custom-api");
    // Path should be prefixed with author name
    assert_eq!(
        doc.languages[0].versions[0].path,
        "custom/docs/custom-api/python"
    );
}

#[test]
fn build_rejects_duplicate_ids() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");

    // Create two DOC.md files under the same author with the same name
    // but in different subdirectories to trigger duplicate IDs
    let dir1 = content_dir.join("acme").join("docs").join("api").join("v1");
    let dir2 = content_dir
        .join("acme")
        .join("docs2")
        .join("api")
        .join("v1");
    std::fs::create_dir_all(&dir1).unwrap();
    std::fs::create_dir_all(&dir2).unwrap();

    // Both have name "api" -> both become "acme/api"
    std::fs::write(
        dir1.join("DOC.md"),
        minimal_doc_frontmatter("api", "javascript", "1.0.0"),
    )
    .unwrap();
    // Second one with different language will be aggregated, not duplicated.
    // To truly duplicate, we need two separate authors generating the same ID.

    // Actually, within one author, same name is aggregated (multi-lang).
    // Duplicates arise across authors. Let's use author-level registry.json
    // that produces the same ID.
    let custom1_dir = content_dir.join("author1");
    let custom2_dir = content_dir.join("author2");
    std::fs::create_dir_all(&custom1_dir).unwrap();
    std::fs::create_dir_all(&custom2_dir).unwrap();

    // Both registries produce "same-id/api"
    let reg = serde_json::json!({
        "docs": [{
            "id": "shared/api",
            "name": "api",
            "description": "API doc",
            "source": "community",
            "tags": [],
            "languages": [{
                "language": "javascript",
                "recommendedVersion": "1.0.0",
                "versions": [{
                    "version": "1.0.0",
                    "path": "docs/api/js",
                    "files": ["DOC.md"],
                    "size": 50,
                    "lastUpdated": "2025-01-01"
                }]
            }]
        }],
        "skills": []
    });
    std::fs::write(
        custom1_dir.join("registry.json"),
        serde_json::to_string(&reg).unwrap(),
    )
    .unwrap();
    std::fs::write(
        custom2_dir.join("registry.json"),
        serde_json::to_string(&reg).unwrap(),
    )
    .unwrap();

    let err = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Duplicate doc id"),
        "Expected duplicate ID error, got: {}",
        msg
    );
}

#[test]
fn build_rejects_missing_required_fields() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");

    // DOC.md missing metadata.languages
    let doc_dir = content_dir.join("acme").join("docs").join("broken");
    std::fs::create_dir_all(&doc_dir).unwrap();
    std::fs::write(
        doc_dir.join("DOC.md"),
        r#"---
name: broken-doc
description: "Missing languages"
metadata:
  versions: "1.0.0"
  source: "community"
---
# Broken
"#,
    )
    .unwrap();

    let err = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("missing 'metadata.languages'"),
        "Expected missing languages error, got: {}",
        msg
    );
}

#[test]
fn build_validate_only_does_not_write_output() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");
    let dist_dir = tmp.path().join("dist");

    write_doc_md(
        &content_dir,
        "acme",
        "widgets",
        "javascript",
        &minimal_doc_frontmatter("widgets", "javascript", "1.0.0"),
    );

    // Call build_content_dir (which never writes files)
    let result = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap();

    // Verify we got valid results
    assert_eq!(result.registry.docs.len(), 1);
    assert_eq!(result.registry.skills.len(), 0);

    // Do NOT call write_build_output (simulating --validate-only)
    // Verify output dir was NOT created
    assert!(
        !dist_dir.exists(),
        "Output directory should not exist in validate-only mode"
    );
}

#[test]
fn build_bm25_index_matches_expected_shape() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");

    // Create multiple docs and a skill
    write_doc_md(
        &content_dir,
        "acme",
        "widgets",
        "javascript",
        &minimal_doc_frontmatter("widgets", "javascript", "2.0.0"),
    );
    write_doc_md(
        &content_dir,
        "acme",
        "payments",
        "python",
        &minimal_doc_frontmatter("payments", "python", "1.0.0"),
    );
    write_skill_md(
        &content_dir,
        "acme",
        "deploy",
        &minimal_skill_frontmatter("deploy"),
    );

    let result = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap();

    // Serialize search index to JSON value and check shape
    let idx_value = serde_json::to_value(&result.search_index).unwrap();
    assert_eq!(idx_value["version"], "1.0.0");
    assert_eq!(idx_value["algorithm"], "bm25");
    assert!(idx_value.get("params").is_some());
    assert!(idx_value["params"].get("k1").is_some());
    assert!(idx_value["params"].get("b").is_some());
    assert_eq!(idx_value["totalDocs"], 3); // 2 docs + 1 skill
    assert!(idx_value.get("avgFieldLengths").is_some());
    assert!(idx_value["avgFieldLengths"].get("name").is_some());
    assert!(idx_value["avgFieldLengths"].get("description").is_some());
    assert!(idx_value["avgFieldLengths"].get("tags").is_some());
    assert!(idx_value.get("idf").is_some());
    assert!(idx_value.get("documents").is_some());
    assert_eq!(idx_value["documents"].as_array().unwrap().len(), 3);
}

#[test]
fn build_json_output_summary() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");

    write_doc_md(
        &content_dir,
        "acme",
        "widgets",
        "javascript",
        &minimal_doc_frontmatter("widgets", "javascript", "1.0.0"),
    );

    let result = build::build_content_dir(&content_dir, "2025-01-01T00:00:00Z", None).unwrap();

    // Simulate JSON summary output (as the command layer would produce)
    let output_dir = tmp.path().join("dist").to_string_lossy().to_string();
    let summary = serde_json::json!({
        "docs": result.registry.docs.len(),
        "skills": result.registry.skills.len(),
        "warnings": result.warnings.len(),
        "output": output_dir,
    });

    // Verify required fields present
    assert!(summary.get("docs").is_some());
    assert!(summary.get("skills").is_some());
    assert!(summary.get("warnings").is_some());
    assert!(summary.get("output").is_some());
    assert_eq!(summary["docs"], 1);
    assert_eq!(summary["skills"], 0);
    assert_eq!(summary["warnings"], 0);
}

#[test]
fn build_outputs_are_deterministic_for_same_input() {
    let tmp = TempDir::new().unwrap();
    let content_dir = tmp.path().join("content");

    // Create a content directory with multiple entries
    write_doc_md(
        &content_dir,
        "zeta",
        "api",
        "python",
        &minimal_doc_frontmatter("api", "python", "2.0.0"),
    );
    write_doc_md(
        &content_dir,
        "alpha",
        "sdk",
        "javascript",
        &minimal_doc_frontmatter("sdk", "javascript", "1.0.0"),
    );
    write_skill_md(
        &content_dir,
        "alpha",
        "deploy",
        &minimal_skill_frontmatter("deploy"),
    );

    let dist1 = tmp.path().join("dist1");
    let dist2 = tmp.path().join("dist2");
    let fixed_ts = "2025-01-01T00:00:00Z";

    // Build twice
    let result1 = build::build_content_dir(&content_dir, fixed_ts, None).unwrap();
    build::write_build_output(&result1, &content_dir, &dist1).unwrap();

    let result2 = build::build_content_dir(&content_dir, fixed_ts, None).unwrap();
    build::write_build_output(&result2, &content_dir, &dist2).unwrap();

    // Compare registry.json byte-for-byte
    let reg1 = std::fs::read_to_string(dist1.join("registry.json")).unwrap();
    let reg2 = std::fs::read_to_string(dist2.join("registry.json")).unwrap();
    assert_eq!(
        reg1, reg2,
        "registry.json should be byte-identical across builds"
    );

    // Compare search-index.json byte-for-byte
    let idx1 = std::fs::read_to_string(dist1.join("search-index.json")).unwrap();
    let idx2 = std::fs::read_to_string(dist2.join("search-index.json")).unwrap();
    assert_eq!(
        idx1, idx2,
        "search-index.json should be byte-identical across builds"
    );

    // Verify order: alpha should come before zeta in both registries
    let reg: serde_json::Value = serde_json::from_str(&reg1).unwrap();
    let doc_ids: Vec<&str> = reg["docs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(doc_ids, vec!["alpha/sdk", "zeta/api"]);
}
