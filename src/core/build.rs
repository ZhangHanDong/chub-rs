//! Build command core logic: scan content directory, parse frontmatter,
//! generate registry.json and search-index.json.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use super::bm25;
use super::frontmatter;

// ── Build-specific output types ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRegistry {
    pub version: String,
    pub generated: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub docs: Vec<BuildDocEntry>,
    pub skills: Vec<BuildSkillEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildDocEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub tags: Vec<String>,
    pub languages: Vec<BuildLanguage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildLanguage {
    pub language: String,
    pub versions: Vec<BuildVersion>,
    #[serde(rename = "recommendedVersion")]
    pub recommended_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildVersion {
    pub version: String,
    pub path: String,
    pub files: Vec<String>,
    pub size: u64,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSkillEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub tags: Vec<String>,
    pub path: String,
    pub files: Vec<String>,
    pub size: u64,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
}

// ── Build result ────────────────────────────────────────────────────

#[derive(Debug)]
pub struct BuildResult {
    pub registry: BuildRegistry,
    pub search_index: bm25::SearchIndex,
    pub warnings: Vec<String>,
    pub author_dirs: Vec<String>,
}

// ── Internal types ──────────────────────────────────────────────────

struct AuthorResult {
    docs: Vec<BuildDocEntry>,
    skills: Vec<BuildSkillEntry>,
    warnings: Vec<String>,
    errors: Vec<String>,
}

struct EntryFile {
    path: std::path::PathBuf,
    rel_path: String,
    entry_type: EntryType,
}

enum EntryType {
    Doc,
    Skill,
}

struct DocBuilder {
    id: String,
    name: String,
    description: String,
    source: String,
    tags: Vec<String>,
    languages: BTreeMap<String, Vec<BuildVersion>>,
}

// ── Public API ──────────────────────────────────────────────────────

/// Scan a content directory and produce a build result (registry + search index).
pub fn build_content_dir(
    content_dir: &Path,
    generated: &str,
    base_url: Option<&str>,
) -> Result<BuildResult> {
    let mut all_docs = Vec::new();
    let mut all_skills = Vec::new();
    let mut all_warnings = Vec::new();
    let mut all_errors = Vec::new();
    let mut author_dirs = Vec::new();

    // List top-level directories (authors), sorted for determinism
    let mut top_level: Vec<_> = std::fs::read_dir(content_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            e.path().is_dir() && name != "dist" && !name.starts_with('.')
        })
        .collect();
    top_level.sort_by_key(|a| a.file_name());

    for author_entry in &top_level {
        let author_name = author_entry.file_name().to_string_lossy().to_string();
        let author_dir = author_entry.path();
        let author_registry = author_dir.join("registry.json");
        author_dirs.push(author_name.clone());

        if author_registry.exists() {
            match load_author_registry(&author_registry, &author_name) {
                Ok((docs, skills)) => {
                    all_docs.extend(docs);
                    all_skills.extend(skills);
                }
                Err(e) => {
                    all_errors.push(format!("{}/registry.json: {}", author_name, e));
                }
            }
        } else {
            let result = discover_author(&author_dir, &author_name, content_dir);
            all_docs.extend(result.docs);
            all_skills.extend(result.skills);
            all_warnings.extend(result.warnings);
            all_errors.extend(result.errors);
        }
    }

    // Check for duplicate IDs
    let mut doc_ids = std::collections::HashSet::new();
    for doc in &all_docs {
        if !doc_ids.insert(&doc.id) {
            all_errors.push(format!("Duplicate doc id '{}'", doc.id));
        }
    }
    let mut skill_ids = std::collections::HashSet::new();
    for skill in &all_skills {
        if !skill_ids.insert(&skill.id) {
            all_errors.push(format!("Duplicate skill id '{}'", skill.id));
        }
    }

    if !all_errors.is_empty() {
        bail!("{}", all_errors.join("\n"));
    }

    // Sort for determinism
    all_docs.sort_by(|a, b| a.id.cmp(&b.id));
    all_skills.sort_by(|a, b| a.id.cmp(&b.id));

    // Build BM25 search index
    let index_entries: Vec<bm25::IndexEntry> = all_docs
        .iter()
        .map(|d| bm25::IndexEntry {
            id: d.id.clone(),
            name: d.name.clone(),
            description: d.description.clone(),
            tags: d.tags.clone(),
        })
        .chain(all_skills.iter().map(|s| bm25::IndexEntry {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            tags: s.tags.clone(),
        }))
        .collect();

    let search_index = bm25::build_index(&index_entries);

    let registry = BuildRegistry {
        version: "1.0.0".to_string(),
        generated: generated.to_string(),
        base_url: base_url.map(|s| s.to_string()),
        docs: all_docs,
        skills: all_skills,
    };

    Ok(BuildResult {
        registry,
        search_index,
        warnings: all_warnings,
        author_dirs,
    })
}

/// Write the build output to the given directory.
pub fn write_build_output(
    result: &BuildResult,
    content_dir: &Path,
    output_dir: &Path,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    // Write registry.json (pretty-printed, deterministic key order via to_value)
    let reg_value = serde_json::to_value(&result.registry)?;
    let registry_json = serde_json::to_string_pretty(&reg_value)?;
    std::fs::write(output_dir.join("registry.json"), registry_json)?;

    // Write search-index.json (compact, deterministic key order via to_value)
    let idx_value = serde_json::to_value(&result.search_index)?;
    let search_json = serde_json::to_string(&idx_value)?;
    std::fs::write(output_dir.join("search-index.json"), search_json)?;

    // Copy content tree (skip registry.json in author dirs)
    for author_name in &result.author_dirs {
        let src = content_dir.join(author_name);
        let dest = output_dir.join(author_name);
        copy_dir_recursive(&src, &dest)?;
    }

    Ok(())
}

// ── Author registry loading ─────────────────────────────────────────

fn load_author_registry(
    path: &Path,
    author_name: &str,
) -> Result<(Vec<BuildDocEntry>, Vec<BuildSkillEntry>)> {
    let content = std::fs::read_to_string(path)?;
    let mut reg: serde_json::Value = serde_json::from_str(&content)?;

    let mut docs = Vec::new();
    let mut skills = Vec::new();

    if let Some(reg_docs) = reg.get_mut("docs").and_then(|v| v.as_array_mut()) {
        for doc_val in reg_docs {
            // Fix ID: prefix with author if needed
            if let Some(id) = doc_val.get("id").and_then(|v| v.as_str()) {
                if !id.contains('/') {
                    doc_val["id"] = serde_json::Value::String(format!("{}/{}", author_name, id));
                }
            } else if let Some(name) = doc_val.get("name").and_then(|v| v.as_str()) {
                doc_val["id"] = serde_json::Value::String(format!("{}/{}", author_name, name));
            }
            // Prefix paths in language versions
            if let Some(languages) = doc_val.get_mut("languages").and_then(|v| v.as_array_mut()) {
                for lang in languages {
                    if let Some(versions) = lang.get_mut("versions").and_then(|v| v.as_array_mut())
                    {
                        for ver in versions {
                            if let Some(p) = ver.get("path").and_then(|v| v.as_str()) {
                                ver["path"] =
                                    serde_json::Value::String(format!("{}/{}", author_name, p));
                            }
                        }
                    }
                }
            }
            let doc: BuildDocEntry = serde_json::from_value(doc_val.clone())?;
            docs.push(doc);
        }
    }

    if let Some(reg_skills) = reg.get_mut("skills").and_then(|v| v.as_array_mut()) {
        for skill_val in reg_skills {
            // Fix ID
            if let Some(id) = skill_val.get("id").and_then(|v| v.as_str()) {
                if !id.contains('/') {
                    skill_val["id"] = serde_json::Value::String(format!("{}/{}", author_name, id));
                }
            } else if let Some(name) = skill_val.get("name").and_then(|v| v.as_str()) {
                skill_val["id"] = serde_json::Value::String(format!("{}/{}", author_name, name));
            }
            // Prefix path
            if let Some(p) = skill_val.get("path").and_then(|v| v.as_str()) {
                skill_val["path"] = serde_json::Value::String(format!("{}/{}", author_name, p));
            }
            let skill: BuildSkillEntry = serde_json::from_value(skill_val.clone())?;
            skills.push(skill);
        }
    }

    Ok((docs, skills))
}

// ── Auto-discovery ──────────────────────────────────────────────────

fn discover_author(author_dir: &Path, author_name: &str, content_dir: &Path) -> AuthorResult {
    let entry_files = find_entry_files(author_dir, author_dir);
    let mut docs: BTreeMap<String, DocBuilder> = BTreeMap::new();
    let mut skills: BTreeMap<String, BuildSkillEntry> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    for ef in entry_files {
        let content = match std::fs::read_to_string(&ef.path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: {}", ef.rel_path, e));
                continue;
            }
        };
        let fm = frontmatter::parse_frontmatter(&content);
        let attrs = &fm.attributes;

        let name = match attrs.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                errors.push(format!("{}: missing 'name' in frontmatter", ef.rel_path));
                continue;
            }
        };

        let description = attrs
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if description.is_empty() {
            warnings.push(format!(
                "{}: missing 'description' in frontmatter",
                ef.rel_path
            ));
        }

        let meta = attrs.get("metadata");
        let source_val = meta
            .and_then(|m| m.get("source"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let source = if source_val.is_empty() {
            warnings.push(format!(
                "{}: missing 'metadata.source', defaulting to 'community'",
                ef.rel_path
            ));
            "community".to_string()
        } else {
            source_val.to_string()
        };

        let tags: Vec<String> = meta
            .and_then(|m| m.get("tags"))
            .and_then(|v| v.as_str())
            .map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let updated_on = meta
            .and_then(|m| m.get("updated-on"))
            .and_then(|v| v.as_str())
            .unwrap_or("2025-01-01")
            .to_string();

        let entry_dir = ef.path.parent().unwrap_or(author_dir);
        let entry_path = relative_path(content_dir, entry_dir);
        let files = list_dir_files(entry_dir);
        let size = dir_size(entry_dir);

        match ef.entry_type {
            EntryType::Skill => {
                if skills.contains_key(&name) {
                    errors.push(format!("{}: duplicate skill name '{}'", ef.rel_path, name));
                    continue;
                }
                skills.insert(
                    name.clone(),
                    BuildSkillEntry {
                        id: format!("{}/{}", author_name, name),
                        name,
                        description,
                        source,
                        tags,
                        path: entry_path,
                        files,
                        size,
                        last_updated: updated_on,
                    },
                );
            }
            EntryType::Doc => {
                let languages_str = meta
                    .and_then(|m| m.get("languages"))
                    .and_then(|v| v.as_str());
                let versions_str = meta
                    .and_then(|m| m.get("versions"))
                    .and_then(|v| v.as_str());

                let languages: Vec<String> = match languages_str {
                    Some(l) if !l.trim().is_empty() => l
                        .split(',')
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect(),
                    _ => {
                        errors.push(format!(
                            "{}: missing 'metadata.languages' in frontmatter",
                            ef.rel_path
                        ));
                        continue;
                    }
                };

                let versions: Vec<String> = match versions_str {
                    Some(v) if !v.trim().is_empty() => v
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                    _ => {
                        errors.push(format!(
                            "{}: missing 'metadata.versions' in frontmatter",
                            ef.rel_path
                        ));
                        continue;
                    }
                };

                let doc = docs.entry(name.clone()).or_insert_with(|| DocBuilder {
                    id: format!("{}/{}", author_name, &name),
                    name: name.clone(),
                    description: description.clone(),
                    source: source.clone(),
                    tags: tags.clone(),
                    languages: BTreeMap::new(),
                });

                for lang in &languages {
                    let lang_versions = doc.languages.entry(lang.clone()).or_default();
                    for ver in &versions {
                        lang_versions.push(BuildVersion {
                            version: ver.clone(),
                            path: entry_path.clone(),
                            files: files.clone(),
                            size,
                            last_updated: updated_on.clone(),
                        });
                    }
                }
            }
        }
    }

    // Convert docs map to array
    let docs_array: Vec<BuildDocEntry> = docs
        .into_values()
        .map(|db| {
            let languages: Vec<BuildLanguage> = db
                .languages
                .into_iter()
                .map(|(lang, mut versions)| {
                    versions.sort_by(|a, b| version_cmp_desc(&a.version, &b.version));
                    let recommended = versions
                        .first()
                        .map(|v| v.version.clone())
                        .unwrap_or_default();
                    BuildLanguage {
                        language: lang,
                        versions,
                        recommended_version: recommended,
                    }
                })
                .collect();
            BuildDocEntry {
                id: db.id,
                name: db.name,
                description: db.description,
                source: db.source,
                tags: db.tags,
                languages,
            }
        })
        .collect();

    AuthorResult {
        docs: docs_array,
        skills: skills.into_values().collect(),
        warnings,
        errors,
    }
}

// ── File system helpers ─────────────────────────────────────────────

fn find_entry_files(dir: &Path, base: &Path) -> Vec<EntryFile> {
    let mut results = Vec::new();
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return results,
    };
    entries.sort_by_key(|a| a.file_name());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            results.extend(find_entry_files(&path, base));
        } else {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "DOC.md" || name == "SKILL.md" {
                let rel = relative_path(base, &path);
                let entry_type = if name == "SKILL.md" {
                    EntryType::Skill
                } else {
                    EntryType::Doc
                };
                results.push(EntryFile {
                    path,
                    rel_path: rel,
                    entry_type,
                });
            }
        }
    }
    results
}

fn list_dir_files(dir: &Path) -> Vec<String> {
    let mut results = Vec::new();
    walk_dir_files(dir, dir, &mut results);
    results.sort();
    results
}

fn walk_dir_files(dir: &Path, base: &Path, results: &mut Vec<String>) {
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|a| a.file_name());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_dir_files(&path, base, results);
        } else {
            let rel = relative_path(base, &path);
            results.push(rel);
        }
    }
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path);
            } else if let Ok(meta) = path.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

fn relative_path(base: &Path, target: &Path) -> String {
    target
        .strip_prefix(base)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| target.to_string_lossy().to_string())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        // Skip registry.json files
        if entry.file_name() == "registry.json" {
            continue;
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

/// Sort versions descending (numeric-aware).
fn version_cmp_desc(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<u64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let b_parts: Vec<u64> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    b_parts.cmp(&a_parts)
}
