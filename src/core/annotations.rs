//! Annotation storage — CRUD operations.
//!
//! Each annotation is stored as a JSON file under `~/.chub/annotations/`.
//! Entry IDs have `/` replaced with `--` for safe filenames.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::config::get_chub_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub note: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// Get the annotations directory path.
pub fn get_annotations_dir() -> PathBuf {
    get_chub_dir().join("annotations")
}

/// Get the annotations directory path under a specific chub dir.
pub fn get_annotations_dir_in(chub_dir: &Path) -> PathBuf {
    chub_dir.join("annotations")
}

/// Convert entry ID to safe filename (replace `/` with `--`).
fn safe_filename(entry_id: &str) -> String {
    format!("{}.json", entry_id.replace('/', "--"))
}

/// Read annotation for an entry. Returns None if not found.
pub fn read_annotation(entry_id: &str) -> Result<Option<Annotation>> {
    read_annotation_in(&get_annotations_dir(), entry_id)
}

/// Read annotation from a specific annotations directory.
pub fn read_annotation_in(annotations_dir: &Path, entry_id: &str) -> Result<Option<Annotation>> {
    let path = annotations_dir.join(safe_filename(entry_id));
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let ann: Annotation = serde_json::from_str(&content)?;
    Ok(Some(ann))
}

/// Write (create or replace) an annotation.
pub fn write_annotation_in(
    annotations_dir: &Path,
    entry_id: &str,
    note: &str,
    now: &str,
) -> Result<Annotation> {
    std::fs::create_dir_all(annotations_dir)?;
    let ann = Annotation {
        id: entry_id.to_string(),
        note: note.to_string(),
        updated_at: now.to_string(),
    };
    let path = annotations_dir.join(safe_filename(entry_id));
    let json = serde_json::to_string_pretty(&ann)?;
    std::fs::write(&path, json)?;
    Ok(ann)
}

/// Clear (delete) an annotation. Returns true if file existed.
pub fn clear_annotation_in(annotations_dir: &Path, entry_id: &str) -> Result<bool> {
    let path = annotations_dir.join(safe_filename(entry_id));
    if path.exists() {
        std::fs::remove_file(&path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// List all annotations from a specific directory.
pub fn list_annotations_in(annotations_dir: &Path) -> Result<Vec<Annotation>> {
    if !annotations_dir.exists() {
        return Ok(vec![]);
    }
    let mut result = vec![];
    for entry in std::fs::read_dir(annotations_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json")
            && let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(ann) = serde_json::from_str::<Annotation>(&content)
        {
            result.push(ann);
        }
    }
    Ok(result)
}
