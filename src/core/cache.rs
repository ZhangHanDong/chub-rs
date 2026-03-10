//! Cache layer for fetching doc/skill content, cache management, and registry updates.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ── Path helpers ────────────────────────────────────────────────────

/// Get the source data directory: `{chub_dir}/sources/{source_name}/data`
pub fn get_source_data_dir(chub_dir: &Path, source_name: &str) -> PathBuf {
    chub_dir.join("sources").join(source_name).join("data")
}

/// Get the registry.json path for a source.
pub fn get_source_registry_path(chub_dir: &Path, source_name: &str) -> PathBuf {
    chub_dir
        .join("sources")
        .join(source_name)
        .join("registry.json")
}

/// Get the search-index.json path for a source.
pub fn get_source_search_index_path(chub_dir: &Path, source_name: &str) -> PathBuf {
    chub_dir
        .join("sources")
        .join(source_name)
        .join("search-index.json")
}

/// Get the meta.json path for a source.
pub fn get_source_meta_path(chub_dir: &Path, source_name: &str) -> PathBuf {
    chub_dir.join("sources").join(source_name).join("meta.json")
}

// ── Meta data ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceMeta {
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<u64>,
    #[serde(rename = "fullBundle", skip_serializing_if = "Option::is_none")]
    pub full_bundle: Option<bool>,
    #[serde(rename = "bundledSeed", skip_serializing_if = "Option::is_none")]
    pub bundled_seed: Option<bool>,
}

/// Read meta.json for a source. Returns default if missing.
pub fn read_meta(chub_dir: &Path, source_name: &str) -> SourceMeta {
    let path = get_source_meta_path(chub_dir, source_name);
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        SourceMeta::default()
    }
}

/// Write meta.json for a source.
pub fn write_meta(chub_dir: &Path, source_name: &str, meta: &SourceMeta) -> Result<()> {
    let path = get_source_meta_path(chub_dir, source_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(meta)?;
    std::fs::write(&path, json)?;
    Ok(())
}

// ── Doc fetching (read path) ────────────────────────────────────────

/// Fetch a doc file content. Tries local source path first, then cached data dir.
pub fn fetch_doc(
    chub_dir: &Path,
    source_path: Option<&Path>,
    source_name: &str,
    doc_path: &str,
    file_name: &str,
) -> Result<String> {
    // 1. Try local source path
    if let Some(local_path) = source_path {
        let full = local_path.join(doc_path).join(file_name);
        if full.exists() {
            return std::fs::read_to_string(&full)
                .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", full.display(), e));
        }
    }

    // 2. Try cached data dir
    let data_dir = get_source_data_dir(chub_dir, source_name);
    let cached = data_dir.join(doc_path).join(file_name);
    if cached.exists() {
        return std::fs::read_to_string(&cached)
            .map_err(|e| anyhow::anyhow!("Failed to read cached {}: {}", cached.display(), e));
    }

    bail!(
        "Could not find '{}' in '{}'. Run `chub update` to download content.",
        file_name,
        doc_path
    )
}

/// Fetch multiple files for --full mode.
pub fn fetch_doc_full(
    chub_dir: &Path,
    source_path: Option<&Path>,
    source_name: &str,
    doc_path: &str,
    files: &[String],
) -> Result<Vec<(String, String)>> {
    let mut result = Vec::new();
    for file in files {
        let content = fetch_doc(chub_dir, source_path, source_name, doc_path, file)?;
        result.push((file.clone(), content));
    }
    Ok(result)
}

// ── Cache status ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub exists: bool,
    pub sources: Vec<SourceCacheInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceCacheInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(rename = "hasRegistry")]
    pub has_registry: bool,
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<u64>,
    #[serde(rename = "fullBundle", skip_serializing_if = "Option::is_none")]
    pub full_bundle: Option<bool>,
    #[serde(rename = "fileCount")]
    pub file_count: usize,
    #[serde(rename = "dataSize")]
    pub data_size: u64,
}

/// Get cache stats for all configured sources.
pub fn get_cache_stats(
    chub_dir: &Path,
    source_names: &[(&str, bool)], // (name, is_local)
) -> CacheStats {
    let sources_dir = chub_dir.join("sources");
    let exists = sources_dir.exists();

    let sources = source_names
        .iter()
        .map(|(name, is_local)| {
            let has_registry = get_source_registry_path(chub_dir, name).exists();
            let meta = read_meta(chub_dir, name);
            let data_dir = get_source_data_dir(chub_dir, name);

            let (file_count, data_size) = if data_dir.exists() {
                count_dir_recursive(&data_dir)
            } else {
                (0, 0)
            };

            SourceCacheInfo {
                name: name.to_string(),
                source_type: if *is_local {
                    "local".to_string()
                } else {
                    "remote".to_string()
                },
                has_registry,
                last_updated: meta.last_updated,
                full_bundle: meta.full_bundle,
                file_count,
                data_size,
            }
        })
        .collect();

    CacheStats { exists, sources }
}

fn count_dir_recursive(dir: &Path) -> (usize, u64) {
    let mut count = 0usize;
    let mut size = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let (c, s) = count_dir_recursive(&path);
                count += c;
                size += s;
            } else if let Ok(meta) = path.metadata() {
                count += 1;
                size += meta.len();
            }
        }
    }
    (count, size)
}

// ── Cache clear ─────────────────────────────────────────────────────

/// Clear all cache except config.yaml.
pub fn clear_cache(chub_dir: &Path) -> Result<()> {
    if !chub_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(chub_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();

        // Preserve config.yaml
        if name == "config.yaml" {
            continue;
        }

        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

// ── Registry update ─────────────────────────────────────────────────

/// Check if cache is fresh based on refresh_interval.
pub fn is_cache_fresh(chub_dir: &Path, source_name: &str, refresh_interval: u64, now: u64) -> bool {
    let meta = read_meta(chub_dir, source_name);
    match meta.last_updated {
        Some(last) => now.saturating_sub(last) < refresh_interval,
        None => false,
    }
}

/// Fetch a remote registry and save it. Uses injected fetcher for testability.
pub fn fetch_and_save_registry<F>(
    chub_dir: &Path,
    source_name: &str,
    fetcher: F,
    now: u64,
) -> Result<()>
where
    F: FnOnce() -> Result<(String, Option<String>)>, // returns (registry_json, optional search_index_json)
{
    let (registry_json, search_index_json) = fetcher()?;

    let source_dir = chub_dir.join("sources").join(source_name);
    std::fs::create_dir_all(&source_dir)?;

    std::fs::write(source_dir.join("registry.json"), &registry_json)?;
    if let Some(idx_json) = search_index_json {
        std::fs::write(source_dir.join("search-index.json"), &idx_json)?;
    }

    let mut meta = read_meta(chub_dir, source_name);
    meta.last_updated = Some(now);
    write_meta(chub_dir, source_name, &meta)?;

    Ok(())
}

/// Extract a tar.gz bundle into the source data directory.
pub fn extract_bundle(chub_dir: &Path, source_name: &str, bundle_data: &[u8]) -> Result<()> {
    let data_dir = get_source_data_dir(chub_dir, source_name);
    std::fs::create_dir_all(&data_dir)?;

    let decoder = flate2::read::GzDecoder::new(bundle_data);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(&data_dir)?;

    // Update meta
    let mut meta = read_meta(chub_dir, source_name);
    meta.full_bundle = Some(true);
    write_meta(chub_dir, source_name, &meta)?;

    Ok(())
}
