//! MCP tool and resource handler implementations.
//!
//! Each handler wraps existing core/ functions and returns MCP-compatible results.

use crate::core::annotations;
use crate::core::cache;
use crate::core::registry::{self, Entry, SearchFilters};

use super::McpContext;

// ── Result helpers ──────────────────────────────────────────────────

fn text_result(data: serde_json::Value) -> serde_json::Value {
    let text = match data {
        serde_json::Value::String(s) => s,
        other => serde_json::to_string_pretty(&other).unwrap_or_default(),
    };
    serde_json::json!({
        "content": [{ "type": "text", "text": text }]
    })
}

fn error_result(message: &str, details: serde_json::Value) -> serde_json::Value {
    let mut obj = serde_json::json!({ "error": message });
    if let serde_json::Value::Object(map) = details
        && let serde_json::Value::Object(ref mut o) = obj
    {
        for (k, v) in map {
            o.insert(k, v);
        }
    }
    let text = serde_json::to_string_pretty(&obj).unwrap_or_default();
    serde_json::json!({
        "content": [{ "type": "text", "text": text }],
        "isError": true
    })
}

fn simplify_entry(entry: &Entry) -> serde_json::Value {
    let mut result = serde_json::json!({
        "id": entry.id,
        "name": entry.name,
        "type": entry.entry_type,
        "description": entry.description,
        "tags": entry.tags,
    });
    if let Some(ref langs) = entry.languages {
        result["languages"] = serde_json::json!(
            langs
                .iter()
                .map(|l| l.language.as_str())
                .collect::<Vec<_>>()
        );
    }
    result
}

// ── Tool Handlers ───────────────────────────────────────────────────

pub fn handle_search(ctx: &McpContext, params: &serde_json::Value) -> serde_json::Value {
    let query = params.get("query").and_then(|v| v.as_str());
    let tags_str = params.get("tags").and_then(|v| v.as_str());
    let lang = params.get("lang").and_then(|v| v.as_str());
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let filters = SearchFilters {
        tags: tags_str.map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
        lang: lang.map(|s| s.to_string()),
        limit: None,
    };

    let merged = registry::MergedRegistry {
        entries: ctx.entries.clone(),
        search_index: ctx.search_index.clone(),
    };

    let entries = if let Some(q) = query {
        registry::search_entries(q, &merged, &filters)
    } else {
        registry::apply_filters(&ctx.entries, &filters)
    };

    let sliced: Vec<_> = entries.iter().take(limit).collect();
    text_result(serde_json::json!({
        "results": sliced.iter().map(|e| simplify_entry(e)).collect::<Vec<_>>(),
        "total": entries.len(),
        "showing": sliced.len(),
    }))
}

pub fn handle_get(ctx: &McpContext, params: &serde_json::Value) -> serde_json::Value {
    let id = match params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return error_result("Missing required parameter: id", serde_json::json!({})),
    };
    let lang = params.get("lang").and_then(|v| v.as_str());
    let version = params.get("version").and_then(|v| v.as_str());
    let full = params
        .get("full")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let file = params.get("file").and_then(|v| v.as_str());

    // Validate file parameter early for path traversal
    if let Some(f) = file
        && (f.contains("..") || f.starts_with('/') || f.starts_with('\\'))
    {
        return error_result(
            &format!(
                "Invalid file path: \"{}\". Path traversal is not allowed.",
                f
            ),
            serde_json::json!({}),
        );
    }

    let lookup = registry::get_entry(id, &ctx.entries);

    if lookup.ambiguous {
        return error_result(
            &format!("Ambiguous entry ID \"{}\". Be specific:", id),
            serde_json::json!({ "alternatives": lookup.alternatives }),
        );
    }

    let entry = match lookup.entry {
        Some(e) => e,
        None => {
            return error_result(
                &format!("Entry \"{}\" not found.", id),
                serde_json::json!({ "suggestion": "Use chub_search to find available entries." }),
            );
        }
    };

    let resolved = registry::resolve_doc_path(&entry, lang, version);

    match resolved {
        registry::ResolvedPath::NeedsLanguage { available } => error_result(
            &format!(
                "Multiple languages available for \"{}\". Specify the lang parameter.",
                id
            ),
            serde_json::json!({ "available": available }),
        ),
        registry::ResolvedPath::LanguageNotAvailable {
            requested,
            available,
        } => error_result(
            &format!("Language \"{}\" not available for \"{}\".", requested, id),
            serde_json::json!({ "available": available }),
        ),
        registry::ResolvedPath::VersionNotFound {
            requested,
            available,
        } => error_result(
            &format!("Version \"{}\" not found for \"{}\".", requested, id),
            serde_json::json!({ "available": available }),
        ),
        registry::ResolvedPath::Unresolvable => error_result(
            &format!("Could not resolve path for \"{}\".", id),
            serde_json::json!({}),
        ),
        registry::ResolvedPath::Resolved {
            source,
            path,
            files,
        } => fetch_and_return(ctx, &entry, &source, &path, &files, file, full, "DOC.md"),
        registry::ResolvedPath::SkillPath {
            source,
            path,
            files,
        } => fetch_and_return(ctx, &entry, &source, &path, &files, file, full, "SKILL.md"),
    }
}

#[allow(clippy::too_many_arguments)]
fn fetch_and_return(
    ctx: &McpContext,
    entry: &Entry,
    source: &str,
    path: &str,
    files: &[String],
    file: Option<&str>,
    full: bool,
    entry_file_name: &str,
) -> serde_json::Value {
    // Determine source_path and source_url from config
    let info = ctx.source_info.get(source);
    let source_path: Option<std::path::PathBuf> = info.and_then(|i| i.path.clone());
    let source_url: Option<&str> = info.and_then(|i| i.url.as_deref());

    let content = if let Some(f) = file {
        // Validate file is in the allowed list
        if let Err(e) = registry::validate_file_path(f, files) {
            return error_result(&format!("{}", e), serde_json::json!({}));
        }
        match cache::fetch_doc(
            &ctx.chub_dir,
            source_path.as_deref(),
            source_url,
            source,
            path,
            f,
        ) {
            Ok(c) => c,
            Err(e) => {
                return error_result(
                    &format!("Failed to fetch \"{}\": {}", entry.id, e),
                    serde_json::json!({}),
                );
            }
        }
    } else if full {
        match cache::fetch_doc_full(
            &ctx.chub_dir,
            source_path.as_deref(),
            source_url,
            source,
            path,
            files,
        ) {
            Ok(all_files) => all_files
                .iter()
                .map(|(name, content)| format!("# FILE: {}\n\n{}", name, content))
                .collect::<Vec<_>>()
                .join("\n\n---\n\n"),
            Err(e) => {
                return error_result(
                    &format!("Failed to fetch \"{}\": {}", entry.id, e),
                    serde_json::json!({}),
                );
            }
        }
    } else {
        match cache::fetch_doc(
            &ctx.chub_dir,
            source_path.as_deref(),
            source_url,
            source,
            path,
            entry_file_name,
        ) {
            Ok(c) => c,
            Err(e) => {
                return error_result(
                    &format!("Failed to fetch \"{}\": {}", entry.id, e),
                    serde_json::json!({}),
                );
            }
        }
    };

    // Append annotation if present
    let mut result = content;
    if let Ok(Some(annotation)) = annotations::read_annotation_in(&ctx.annotations_dir, &entry.id) {
        result.push_str(&format!(
            "\n\n---\n[Agent note — {}]\n{}\n",
            annotation.updated_at, annotation.note
        ));
    }

    text_result(serde_json::Value::String(result))
}

pub fn handle_list(ctx: &McpContext, params: &serde_json::Value) -> serde_json::Value {
    let tags_str = params.get("tags").and_then(|v| v.as_str());
    let lang = params.get("lang").and_then(|v| v.as_str());
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let filters = SearchFilters {
        tags: tags_str.map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
        lang: lang.map(|s| s.to_string()),
        limit: None,
    };

    let entries = registry::apply_filters(&ctx.entries, &filters);
    let sliced: Vec<_> = entries.iter().take(limit).collect();

    text_result(serde_json::json!({
        "entries": sliced.iter().map(|e| simplify_entry(e)).collect::<Vec<_>>(),
        "total": entries.len(),
        "showing": sliced.len(),
    }))
}

pub fn handle_annotate(
    ctx: &McpContext,
    params: &serde_json::Value,
    now: &str,
) -> serde_json::Value {
    let list_mode = params
        .get("list")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let id = params.get("id").and_then(|v| v.as_str());
    let note = params.get("note").and_then(|v| v.as_str());
    let clear = params
        .get("clear")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if list_mode {
        let anns = annotations::list_annotations_in(&ctx.annotations_dir).unwrap_or_default();
        return text_result(serde_json::json!({
            "annotations": anns.iter().map(|a| serde_json::json!({
                "id": a.id,
                "note": a.note,
                "updatedAt": a.updated_at,
            })).collect::<Vec<_>>(),
            "total": anns.len(),
        }));
    }

    let id = match id {
        Some(id) => id,
        None => {
            return error_result(
                "Missing required parameter: id. Provide an entry ID or use list mode.",
                serde_json::json!({}),
            );
        }
    };

    // Validate entry ID
    if id.len() > 200 {
        return error_result(
            "Entry ID too long (max 200 characters).",
            serde_json::json!({}),
        );
    }
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '/')
    {
        return error_result(
            "Entry ID contains invalid characters. Use only alphanumeric, hyphens, underscores, dots, and slashes.",
            serde_json::json!({}),
        );
    }

    if clear {
        let removed = annotations::clear_annotation_in(&ctx.annotations_dir, id).unwrap_or(false);
        return text_result(serde_json::json!({
            "status": if removed { "cleared" } else { "not_found" },
            "id": id,
        }));
    }

    if let Some(note_text) = note {
        if let Err(e) = annotations::write_annotation_in(&ctx.annotations_dir, id, note_text, now) {
            return error_result(
                &format!("Failed to save annotation: {}", e),
                serde_json::json!({}),
            );
        }
        let ann = annotations::read_annotation_in(&ctx.annotations_dir, id)
            .ok()
            .flatten();
        return text_result(serde_json::json!({
            "status": "saved",
            "annotation": ann.map(|a| serde_json::json!({
                "id": a.id,
                "note": a.note,
                "updatedAt": a.updated_at,
            })),
        }));
    }

    // Read mode
    match annotations::read_annotation_in(&ctx.annotations_dir, id) {
        Ok(Some(ann)) => text_result(serde_json::json!({
            "annotation": {
                "id": ann.id,
                "note": ann.note,
                "updatedAt": ann.updated_at,
            }
        })),
        _ => text_result(serde_json::json!({
            "status": "no_annotation",
            "id": id,
        })),
    }
}

pub fn handle_feedback(ctx: &McpContext, params: &serde_json::Value) -> serde_json::Value {
    if !ctx.telemetry_enabled {
        return text_result(serde_json::json!({
            "status": "skipped",
            "reason": "telemetry_disabled",
        }));
    }

    let id = match params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return error_result("Missing required parameter: id", serde_json::json!({})),
    };
    let rating = match params.get("rating").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => return error_result("Missing required parameter: rating", serde_json::json!({})),
    };

    // Auto-detect entry type
    let entry_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let lookup = registry::get_entry(id, &ctx.entries);
            if let Some(ref entry) = lookup.entry {
                entry.entry_type.clone()
            } else {
                "doc".to_string()
            }
        });

    let comment = params.get("comment").and_then(|v| v.as_str());
    let lang = params.get("lang").and_then(|v| v.as_str());
    let version = params.get("version").and_then(|v| v.as_str());
    let file = params.get("file").and_then(|v| v.as_str());
    let labels: Vec<String> = params
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Get client ID (best effort)
    let client_id = crate::core::identity::get_or_create_client_id(&ctx.chub_dir)
        .unwrap_or_else(|_| "unknown".to_string());

    let payload = crate::core::telemetry::FeedbackPayload {
        entry_id: id.to_string(),
        entry_type,
        rating: rating.to_string(),
        comment: comment.map(|s| s.to_string()),
        language: lang.map(|s| s.to_string()),
        doc_version: version.map(|s| s.to_string()),
        file: file.map(|s| s.to_string()),
        labels,
        agent: Some("mcp-server".to_string()),
        model: None,
        client_id,
    };

    match crate::core::telemetry::send_feedback(
        &payload,
        &ctx.feedback_endpoint,
        ctx.telemetry_enabled,
    ) {
        Ok(result) => text_result(serde_json::json!({
            "status": format!("{:?}", result),
        })),
        Err(e) => error_result(&format!("Feedback failed: {}", e), serde_json::json!({})),
    }
}

pub fn handle_registry_resource(ctx: &McpContext) -> serde_json::Value {
    let simplified: Vec<serde_json::Value> = ctx
        .entries
        .iter()
        .map(|entry| {
            let mut result = serde_json::json!({
                "id": entry.id,
                "name": entry.name,
                "type": entry.entry_type,
                "description": entry.description,
                "tags": entry.tags,
            });
            if let Some(ref langs) = entry.languages {
                result["languages"] = serde_json::json!(
                langs.iter().map(|l| serde_json::json!({
                    "language": l.language,
                    "versions": l.versions.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
                    "recommended": l.recommended_version,
                })).collect::<Vec<_>>()
            );
            }
            result
        })
        .collect();

    serde_json::json!({
        "contents": [{
            "uri": "chub://registry",
            "mimeType": "application/json",
            "text": serde_json::to_string_pretty(&serde_json::json!({
                "entries": simplified,
                "total": simplified.len(),
            })).unwrap_or_default(),
        }]
    })
}

// ── Tool definitions (for tools/list) ───────────────────────────────

pub fn tool_definitions() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "chub_search",
                "description": "Search Context Hub for docs and skills by query, tags, or language",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query. Omit to list all entries." },
                        "tags": { "type": "string", "description": "Comma-separated tag filter" },
                        "lang": { "type": "string", "description": "Filter by language" },
                        "limit": { "type": "integer", "description": "Max results (default 20)", "minimum": 1, "maximum": 100 }
                    }
                }
            },
            {
                "name": "chub_get",
                "description": "Fetch the content of a doc or skill by ID from Context Hub",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Entry ID (e.g. \"openai/chat\")" },
                        "lang": { "type": "string", "description": "Language variant" },
                        "version": { "type": "string", "description": "Specific version" },
                        "full": { "type": "boolean", "description": "Fetch all files" },
                        "file": { "type": "string", "description": "Fetch a specific file" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "chub_list",
                "description": "List all available docs and skills in Context Hub",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tags": { "type": "string", "description": "Comma-separated tag filter" },
                        "lang": { "type": "string", "description": "Filter by language" },
                        "limit": { "type": "integer", "description": "Max entries (default 50)", "minimum": 1, "maximum": 500 }
                    }
                }
            },
            {
                "name": "chub_annotate",
                "description": "Read, write, clear, or list agent annotations",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Entry ID to annotate" },
                        "note": { "type": "string", "description": "Annotation text to save" },
                        "clear": { "type": "boolean", "description": "Remove the annotation" },
                        "list": { "type": "boolean", "description": "List all annotations" }
                    }
                }
            },
            {
                "name": "chub_feedback",
                "description": "Send quality feedback for a doc or skill",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Entry ID to rate" },
                        "rating": { "type": "string", "enum": ["up", "down"], "description": "Thumbs up or down" },
                        "comment": { "type": "string", "description": "Optional comment" },
                        "type": { "type": "string", "enum": ["doc", "skill"], "description": "Entry type" },
                        "lang": { "type": "string", "description": "Language variant rated" },
                        "version": { "type": "string", "description": "Version rated" },
                        "file": { "type": "string", "description": "Specific file rated" },
                        "labels": { "type": "array", "items": { "type": "string" }, "description": "Feedback labels" }
                    },
                    "required": ["id", "rating"]
                }
            }
        ]
    })
}

pub fn resource_definitions() -> serde_json::Value {
    serde_json::json!({
        "resources": [{
            "uri": "chub://registry",
            "name": "registry",
            "description": "Browse the full Context Hub registry of docs and skills",
            "mimeType": "application/json"
        }]
    })
}
