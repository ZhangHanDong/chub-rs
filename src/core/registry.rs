//! Registry loading, merging, searching and entry resolution.
//!
//! Handles multi-source registries, BM25/keyword search, and doc path resolution.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::bm25::{self, SearchIndex};
use super::normalize::normalize_language;

// ── Data types (compatible with JS registry.json) ───────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: String,
    pub generated: String,
    pub docs: Vec<DocEntry>,
    pub skills: Vec<SkillEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub languages: Vec<Language>,
    /// Added during merge to track source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_sourceObj")]
    pub source_obj: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub language: String,
    #[serde(rename = "recommendedVersion")]
    pub recommended_version: String,
    pub versions: Vec<Version>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub version: String,
    pub path: String,
    pub files: Vec<String>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub path: String,
    pub files: Vec<String>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_sourceObj")]
    pub source_obj: Option<serde_json::Value>,
}

/// Unified entry for search results and display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(rename = "type")]
    pub entry_type: String, // "doc" or "skill"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<Language>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_sourceObj")]
    pub source_obj: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _score: Option<f64>,
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl Entry {
    pub fn from_doc(doc: &DocEntry) -> Self {
        Entry {
            id: doc.id.clone(),
            name: doc.name.clone(),
            description: doc.description.clone(),
            tags: doc.tags.clone(),
            entry_type: "doc".to_string(),
            languages: Some(doc.languages.clone()),
            path: None,
            files: None,
            _source: doc._source.clone(),
            source_obj: doc.source_obj.clone(),
            _score: None,
            last_updated: None,
            size: None,
        }
    }

    pub fn from_skill(skill: &SkillEntry) -> Self {
        Entry {
            id: skill.id.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
            tags: skill.tags.clone(),
            entry_type: "skill".to_string(),
            languages: None,
            path: Some(skill.path.clone()),
            files: Some(skill.files.clone()),
            _source: skill._source.clone(),
            source_obj: skill.source_obj.clone(),
            _score: None,
            last_updated: Some(skill.last_updated.clone()),
            size: Some(skill.size),
        }
    }
}

// ── Merged registry ─────────────────────────────────────────────────

pub struct MergedRegistry {
    pub entries: Vec<Entry>,
    pub search_index: Option<SearchIndex>,
}

/// Load a registry from a JSON file.
pub fn load_registry(path: &Path) -> Result<Registry> {
    let content = std::fs::read_to_string(path)?;
    let reg: Registry = serde_json::from_str(&content)?;
    Ok(reg)
}

/// Load a search index from a JSON file.
pub fn load_search_index(path: &Path) -> Result<SearchIndex> {
    let content = std::fs::read_to_string(path)?;
    let idx: SearchIndex = serde_json::from_str(&content)?;
    Ok(idx)
}

/// Merge entries from multiple registries, tagging each with source info.
pub fn merge_registries(registries: &[(String, Registry, Option<SearchIndex>)]) -> MergedRegistry {
    let mut entries = Vec::new();
    let mut all_index_docs = Vec::new();
    let mut has_any_index = false;

    for (source_name, registry, search_idx) in registries {
        for doc in &registry.docs {
            let mut e = Entry::from_doc(doc);
            e._source = Some(source_name.clone());
            entries.push(e);
        }
        for skill in &registry.skills {
            let mut e = Entry::from_skill(skill);
            e._source = Some(source_name.clone());
            entries.push(e);
        }
        if let Some(idx) = search_idx {
            has_any_index = true;
            for doc in &idx.documents {
                all_index_docs.push(doc.clone());
            }
        }
    }

    // Build a merged search index if any source had one
    let search_index = if has_any_index && !all_index_docs.is_empty() {
        // Recompute IDF across all documents
        let total_docs = all_index_docs.len();
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut total_name = 0usize;
        let mut total_desc = 0usize;
        let mut total_tags = 0usize;

        for doc in &all_index_docs {
            let mut seen = HashSet::new();
            for t in doc
                .tokens
                .name
                .iter()
                .chain(doc.tokens.description.iter())
                .chain(doc.tokens.tags.iter())
            {
                seen.insert(t.clone());
            }
            for t in &seen {
                *df.entry(t.clone()).or_insert(0) += 1;
            }
            total_name += doc.tokens.name.len();
            total_desc += doc.tokens.description.len();
            total_tags += doc.tokens.tags.len();
        }

        let n = total_docs as f64;
        let idf: HashMap<String, f64> = df
            .into_iter()
            .map(|(term, count)| {
                let df_val = count as f64;
                let score = ((n - df_val + 0.5) / (df_val + 0.5) + 1.0).ln();
                (term, score)
            })
            .collect();

        Some(SearchIndex {
            version: "1.0.0".to_string(),
            algorithm: "bm25".to_string(),
            params: bm25::BM25Params { k1: 1.5, b: 0.75 },
            total_docs,
            avg_field_lengths: bm25::FieldLengths {
                name: if total_docs > 0 {
                    total_name as f64 / n
                } else {
                    0.0
                },
                description: if total_docs > 0 {
                    total_desc as f64 / n
                } else {
                    0.0
                },
                tags: if total_docs > 0 {
                    total_tags as f64 / n
                } else {
                    0.0
                },
            },
            idf,
            documents: all_index_docs,
        })
    } else {
        None
    };

    MergedRegistry {
        entries,
        search_index,
    }
}

// ── Search & filtering ──────────────────────────────────────────────

#[derive(Default)]
pub struct SearchFilters {
    pub tags: Option<Vec<String>>,
    pub lang: Option<String>,
    pub limit: Option<usize>,
}

/// Apply tag and language filters to entries.
pub fn apply_filters(entries: &[Entry], filters: &SearchFilters) -> Vec<Entry> {
    let mut result: Vec<Entry> = entries
        .iter()
        .filter(|e| {
            // Tag filter
            if let Some(ref tags) = filters.tags {
                let entry_tags: HashSet<String> = e.tags.iter().map(|t| t.to_lowercase()).collect();
                if !tags.iter().any(|t| entry_tags.contains(&t.to_lowercase())) {
                    return false;
                }
            }
            // Language filter
            if let Some(ref lang) = filters.lang {
                let norm = normalize_language(Some(lang));
                if let Some(ref norm_lang) = norm {
                    if let Some(ref langs) = e.languages
                        && !langs.iter().any(|l| {
                            let entry_lang =
                                normalize_language(Some(&l.language)).unwrap_or_default();
                            entry_lang == *norm_lang
                        })
                    {
                        return false;
                    }
                    // Skills don't have languages, so if lang filter is set, exclude skills
                    if e.languages.is_none() {
                        return false;
                    }
                }
            }
            true
        })
        .cloned()
        .collect();

    if let Some(limit) = filters.limit {
        result.truncate(limit);
    }
    result
}

/// Search entries using BM25 index (if available) or keyword fallback.
pub fn search_entries(query: &str, merged: &MergedRegistry, filters: &SearchFilters) -> Vec<Entry> {
    let mut scored: Vec<Entry> = if let Some(ref index) = merged.search_index {
        // BM25 search
        let results = bm25::search(query, index);
        let mut score_map: HashMap<String, f64> = HashMap::new();
        for r in results {
            let entry = score_map.entry(r.id).or_insert(0.0_f64);
            if r.score > *entry {
                *entry = r.score;
            }
        }

        merged
            .entries
            .iter()
            .filter_map(|e| {
                score_map.get(&e.id).map(|&score| {
                    let mut entry = e.clone();
                    entry._score = Some(score);
                    entry
                })
            })
            .collect()
    } else {
        // Keyword fallback
        keyword_search(query, &merged.entries)
    };

    scored.sort_by(|a, b| {
        b._score
            .unwrap_or(0.0)
            .partial_cmp(&a._score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply filters
    apply_filters(&scored, filters)
}

/// Simple keyword scoring for when no BM25 index is available.
fn keyword_search(query: &str, entries: &[Entry]) -> Vec<Entry> {
    let q = query.to_lowercase();
    let query_words: Vec<&str> = q.split_whitespace().collect();

    entries
        .iter()
        .filter_map(|e| {
            let id_lower = e.id.to_lowercase();
            let name_lower = e.name.to_lowercase();
            let desc_lower = e.description.to_lowercase();

            let mut score = 0.0;

            // Exact ID match
            if id_lower == q {
                score += 100.0;
            } else if id_lower.contains(&q) {
                score += 50.0;
            }

            // Name matching
            if name_lower == q {
                score += 80.0;
            } else if name_lower.contains(&q) {
                score += 40.0;
            }

            // Word matching on tags and description
            for word in &query_words {
                for tag in &e.tags {
                    if tag.to_lowercase().contains(word) {
                        score += 15.0;
                    }
                }
                if desc_lower.contains(word) {
                    score += 5.0;
                }
            }

            if score > 0.0 {
                let mut entry = e.clone();
                entry._score = Some(score);
                Some(entry)
            } else {
                None
            }
        })
        .collect()
}

// ── Entry lookup ────────────────────────────────────────────────────

pub struct EntryLookup {
    pub entry: Option<Entry>,
    pub ambiguous: bool,
    pub alternatives: Vec<String>,
}

/// Get an entry by ID or source:id format.
/// Returns ambiguous result if multiple sources have the same ID.
pub fn get_entry(id_or_namespaced: &str, entries: &[Entry]) -> EntryLookup {
    // Check for source:id format
    if let Some((source, id)) = id_or_namespaced.split_once(':') {
        let matching: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.id == id && e._source.as_deref() == Some(source))
            .collect();
        return EntryLookup {
            entry: matching.first().cloned().cloned(),
            ambiguous: false,
            alternatives: vec![],
        };
    }

    let matching: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.id == id_or_namespaced)
        .collect();

    match matching.len() {
        0 => EntryLookup {
            entry: None,
            ambiguous: false,
            alternatives: vec![],
        },
        1 => EntryLookup {
            entry: Some(matching[0].clone()),
            ambiguous: false,
            alternatives: vec![],
        },
        _ => {
            let alts: Vec<String> = matching
                .iter()
                .filter_map(|e| e._source.as_ref().map(|s| format!("{}:{}", s, e.id)))
                .collect();
            EntryLookup {
                entry: None,
                ambiguous: true,
                alternatives: alts,
            }
        }
    }
}

// ── Doc path resolution ─────────────────────────────────────────────

#[derive(Debug)]
pub enum ResolvedPath {
    /// Successfully resolved doc path.
    Resolved {
        source: String,
        path: String,
        files: Vec<String>,
    },
    /// Skill path (no language needed).
    SkillPath {
        source: String,
        path: String,
        files: Vec<String>,
    },
    /// Language is required — list available languages.
    NeedsLanguage { available: Vec<String> },
    /// Requested language not available.
    LanguageNotAvailable {
        requested: String,
        available: Vec<String>,
    },
    /// Requested version not found.
    VersionNotFound {
        requested: String,
        available: Vec<String>,
    },
    /// Entry has no path or languages.
    Unresolvable,
}

/// Resolve the path for a doc or skill entry given optional language and version.
pub fn resolve_doc_path(
    entry: &Entry,
    language: Option<&str>,
    version: Option<&str>,
) -> ResolvedPath {
    let source = entry._source.clone().unwrap_or_default();

    // Skill entries: use path directly
    if entry.entry_type == "skill" {
        if let Some(ref path) = entry.path {
            return ResolvedPath::SkillPath {
                source,
                path: path.clone(),
                files: entry.files.clone().unwrap_or_default(),
            };
        }
        return ResolvedPath::Unresolvable;
    }

    // Doc entries: need language
    let languages = match entry.languages {
        Some(ref l) if !l.is_empty() => l,
        _ => return ResolvedPath::Unresolvable,
    };

    let available: Vec<String> = languages.iter().map(|l| l.language.clone()).collect();

    // Language is always required for docs (per spec)
    let lang_str = match language {
        Some(l) => l,
        None => {
            return ResolvedPath::NeedsLanguage { available };
        }
    };

    let norm_lang = normalize_language(Some(lang_str)).unwrap_or_else(|| lang_str.to_lowercase());

    let lang = languages.iter().find(|l| {
        let entry_norm =
            normalize_language(Some(&l.language)).unwrap_or_else(|| l.language.to_lowercase());
        entry_norm == norm_lang
    });

    let lang = match lang {
        Some(l) => l,
        None => {
            return ResolvedPath::LanguageNotAvailable {
                requested: lang_str.to_string(),
                available,
            };
        }
    };

    // Version resolution
    let ver = match version {
        Some(v) => {
            // Explicit version: must match exactly
            match lang.versions.iter().find(|ver| ver.version == v) {
                Some(ver) => ver,
                None => {
                    let avail: Vec<String> =
                        lang.versions.iter().map(|v| v.version.clone()).collect();
                    return ResolvedPath::VersionNotFound {
                        requested: v.to_string(),
                        available: avail,
                    };
                }
            }
        }
        None => {
            // Default: use recommendedVersion
            match lang
                .versions
                .iter()
                .find(|v| v.version == lang.recommended_version)
                .or(lang.versions.first())
            {
                Some(v) => v,
                None => return ResolvedPath::Unresolvable,
            }
        }
    };

    ResolvedPath::Resolved {
        source,
        path: ver.path.clone(),
        files: ver.files.clone(),
    }
}

/// Validate a --file path: reject absolute paths, `..` segments, and paths not in the entry's file list.
pub fn validate_file_path(requested: &str, allowed_files: &[String]) -> Result<()> {
    // Reject absolute paths
    if requested.starts_with('/') || requested.starts_with('\\') {
        bail!(
            "Absolute paths are not allowed with --file. Use a relative path from the entry's file list: {}",
            allowed_files.join(", ")
        );
    }

    // Reject path traversal
    if requested.contains("..") {
        bail!(
            "Path traversal is not allowed with --file. Use a relative path from the entry's file list: {}",
            allowed_files.join(", ")
        );
    }

    // Must be in the allowed files list
    if !allowed_files.iter().any(|f| f == requested) {
        bail!(
            "File '{}' not found in entry. Available files: {}",
            requested,
            allowed_files.join(", ")
        );
    }

    Ok(())
}
