#![allow(dead_code)]
/// Shared test helpers for creating fixture registries and temp environments.
use chub_rs::core::registry::{DocEntry, Language, MergedRegistry, Registry, SkillEntry, Version};
use std::path::Path;

/// Create a fixture registry with known test entries.
pub fn fixture_registry() -> Registry {
    Registry {
        version: "1.0.0".to_string(),
        generated: "2025-01-01T00:00:00Z".to_string(),
        docs: vec![
            DocEntry {
                id: "acme/widgets".to_string(),
                name: "widgets".to_string(),
                description: "Acme widget API documentation".to_string(),
                tags: vec![
                    "api".to_string(),
                    "widgets".to_string(),
                    "automation".to_string(),
                ],
                languages: vec![Language {
                    language: "javascript".to_string(),
                    recommended_version: "2.0.0".to_string(),
                    versions: vec![Version {
                        version: "2.0.0".to_string(),
                        path: "acme/widgets/javascript/v2".to_string(),
                        files: vec!["DOC.md".to_string(), "references/advanced.md".to_string()],
                        last_updated: "2025-01-01".to_string(),
                        size: 12345,
                    }],
                }],
                _source: None,
                source_obj: None,
            },
            DocEntry {
                id: "acme/versioned-api".to_string(),
                name: "versioned-api".to_string(),
                description: "Acme versioned API with multiple versions".to_string(),
                tags: vec!["api".to_string(), "versioned".to_string()],
                languages: vec![Language {
                    language: "javascript".to_string(),
                    recommended_version: "2.0.0".to_string(),
                    versions: vec![
                        Version {
                            version: "2.0.0".to_string(),
                            path: "acme/versioned-api/javascript/v2".to_string(),
                            files: vec!["DOC.md".to_string()],
                            last_updated: "2025-01-01".to_string(),
                            size: 8000,
                        },
                        Version {
                            version: "1.0.0".to_string(),
                            path: "acme/versioned-api/javascript/v1".to_string(),
                            files: vec!["DOC.md".to_string()],
                            last_updated: "2024-06-01".to_string(),
                            size: 6000,
                        },
                    ],
                }],
                _source: None,
                source_obj: None,
            },
            DocEntry {
                id: "multilang/client".to_string(),
                name: "client".to_string(),
                description: "Multi-language SDK client documentation".to_string(),
                tags: vec!["sdk".to_string(), "client".to_string()],
                languages: vec![
                    Language {
                        language: "javascript".to_string(),
                        recommended_version: "1.0.0".to_string(),
                        versions: vec![Version {
                            version: "1.0.0".to_string(),
                            path: "multilang/client/javascript".to_string(),
                            files: vec!["DOC.md".to_string()],
                            last_updated: "2025-01-01".to_string(),
                            size: 5000,
                        }],
                    },
                    Language {
                        language: "python".to_string(),
                        recommended_version: "1.0.0".to_string(),
                        versions: vec![Version {
                            version: "1.0.0".to_string(),
                            path: "multilang/client/python".to_string(),
                            files: vec!["DOC.md".to_string()],
                            last_updated: "2025-01-01".to_string(),
                            size: 4500,
                        }],
                    },
                ],
                _source: None,
                source_obj: None,
            },
        ],
        skills: vec![SkillEntry {
            id: "testskills/deploy".to_string(),
            name: "deploy".to_string(),
            description: "Deployment automation skill for CI/CD pipelines".to_string(),
            tags: vec![
                "automation".to_string(),
                "deploy".to_string(),
                "ci-cd".to_string(),
            ],
            path: "testskills/deploy".to_string(),
            files: vec!["SKILL.md".to_string()],
            last_updated: "2025-01-01".to_string(),
            size: 3000,
            _source: None,
            source_obj: None,
        }],
    }
}

/// Create a merged registry from a single fixture source.
pub fn fixture_merged(source_name: &str) -> MergedRegistry {
    let reg = fixture_registry();
    // Build a search index from the entries
    let entries: Vec<chub_rs::core::bm25::IndexEntry> = reg
        .docs
        .iter()
        .map(|d| chub_rs::core::bm25::IndexEntry {
            id: d.id.clone(),
            name: d.name.clone(),
            description: d.description.clone(),
            tags: d.tags.clone(),
        })
        .chain(reg.skills.iter().map(|s| chub_rs::core::bm25::IndexEntry {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            tags: s.tags.clone(),
        }))
        .collect();
    let idx = chub_rs::core::bm25::build_index(&entries);

    chub_rs::core::registry::merge_registries(&[(source_name.to_string(), reg, Some(idx))])
}

/// Create a merged registry with duplicate entries from two sources.
pub fn fixture_merged_ambiguous() -> MergedRegistry {
    let reg1 = Registry {
        version: "1.0.0".to_string(),
        generated: "2025-01-01T00:00:00Z".to_string(),
        docs: vec![DocEntry {
            id: "openai/chat".to_string(),
            name: "chat".to_string(),
            description: "OpenAI Chat API from source 1".to_string(),
            tags: vec!["api".to_string()],
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
        }],
        skills: vec![],
    };

    let reg2 = Registry {
        version: "1.0.0".to_string(),
        generated: "2025-01-01T00:00:00Z".to_string(),
        docs: vec![DocEntry {
            id: "openai/chat".to_string(),
            name: "chat".to_string(),
            description: "OpenAI Chat API from source 2".to_string(),
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
        }],
        skills: vec![],
    };

    chub_rs::core::registry::merge_registries(&[
        ("source1".to_string(), reg1, None),
        ("source2".to_string(), reg2, None),
    ])
}

/// Set up a temp chub directory with cached doc files for testing get commands.
pub fn setup_cached_docs(chub_dir: &Path, source_name: &str) {
    // acme/widgets
    let widget_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("acme/widgets/javascript/v2");
    std::fs::create_dir_all(&widget_dir).expect("create widget dir");
    std::fs::create_dir_all(widget_dir.join("references")).expect("create refs dir");
    std::fs::write(
        widget_dir.join("DOC.md"),
        "# Acme Widgets\n\nWidget documentation content.\n",
    )
    .expect("write DOC.md");
    std::fs::write(
        widget_dir.join("references/advanced.md"),
        "# Advanced Widgets\n\nAdvanced widget operations.\n",
    )
    .expect("write advanced.md");

    // acme/versioned-api v2
    let v2_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("acme/versioned-api/javascript/v2");
    std::fs::create_dir_all(&v2_dir).expect("create v2 dir");
    std::fs::write(
        v2_dir.join("DOC.md"),
        "# API v2.0.0\n\nVersion 2 content.\n",
    )
    .expect("write v2 DOC.md");

    // acme/versioned-api v1
    let v1_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("acme/versioned-api/javascript/v1");
    std::fs::create_dir_all(&v1_dir).expect("create v1 dir");
    std::fs::write(
        v1_dir.join("DOC.md"),
        "# API v1.0.0\n\nVersion 1 content.\n",
    )
    .expect("write v1 DOC.md");

    // multilang/client javascript
    let js_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("multilang/client/javascript");
    std::fs::create_dir_all(&js_dir).expect("create js dir");
    std::fs::write(
        js_dir.join("DOC.md"),
        "# Client SDK (JavaScript)\n\nJS client content.\n",
    )
    .expect("write js DOC.md");

    // multilang/client python
    let py_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("multilang/client/python");
    std::fs::create_dir_all(&py_dir).expect("create py dir");
    std::fs::write(
        py_dir.join("DOC.md"),
        "# Client SDK (Python)\n\nPython client content.\n",
    )
    .expect("write py DOC.md");

    // testskills/deploy
    let skill_dir = chub_dir
        .join("sources")
        .join(source_name)
        .join("data")
        .join("testskills/deploy");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "# Deploy Skill\n\nDeployment automation steps.\n",
    )
    .expect("write SKILL.md");
}

/// Write an annotation file for testing.
pub fn write_test_annotation(annotations_dir: &Path, entry_id: &str, note: &str) {
    std::fs::create_dir_all(annotations_dir).expect("create annotations dir");
    let safe_name = format!("{}.json", entry_id.replace('/', "--"));
    let annotation = serde_json::json!({
        "id": entry_id,
        "note": note,
        "updatedAt": "2025-01-01T00:00:00Z"
    });
    std::fs::write(
        annotations_dir.join(safe_name),
        serde_json::to_string_pretty(&annotation).expect("serialize"),
    )
    .expect("write annotation");
}
