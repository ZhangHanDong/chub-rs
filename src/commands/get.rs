use anyhow::{Result, bail};
use clap::Args;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::annotations;
use crate::core::cache;
use crate::core::registry::{
    self, Entry, MergedRegistry, ResolvedPath, get_entry, validate_file_path,
};

#[derive(Args, Clone)]
pub struct GetArgs {
    /// Entry IDs to fetch (e.g. "openai/chat", "stripe/api")
    #[arg(required = true)]
    pub ids: Vec<String>,

    /// Language variant (required for docs): py, js, ts, rb, cs
    #[arg(long)]
    pub lang: Option<String>,

    /// Specific version (for docs)
    #[arg(long)]
    pub version: Option<String>,

    /// Write to file or directory
    #[arg(short, long)]
    pub output: Option<String>,

    /// Fetch all files (not just entry point)
    #[arg(long)]
    pub full: bool,

    /// Fetch specific file(s) by path (comma-separated)
    #[arg(long)]
    pub file: Option<String>,
}

/// Run the get command.
pub fn run(
    args: &GetArgs,
    merged: &MergedRegistry,
    chub_dir: &Path,
    annotations_dir: &Path,
    source_paths: &HashMap<String, Option<PathBuf>>,
    json: bool,
) -> Result<()> {
    if args.ids.is_empty() {
        bail!("No entry IDs specified. Usage: chub get <id> [id2 ...]");
    }

    let mut all_outputs: Vec<serde_json::Value> = Vec::new();
    let multiple = args.ids.len() > 1;

    for id in &args.ids {
        let lookup = get_entry(id, &merged.entries);

        if lookup.ambiguous {
            let msg = format!(
                "Multiple entries found for '{}'. Specify the source:\n{}",
                id,
                lookup
                    .alternatives
                    .iter()
                    .map(|a| format!("  chub get {}", a))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            if json {
                all_outputs.push(serde_json::json!({"error": msg, "id": id}));
                continue;
            }
            bail!("{}", msg);
        }

        let entry = match lookup.entry {
            Some(e) => e,
            None => {
                let msg = format!("No doc or skill found with id '{}'", id);
                if json {
                    all_outputs.push(serde_json::json!({"error": msg, "id": id}));
                    continue;
                }
                bail!("{}", msg);
            }
        };

        let result =
            fetch_entry_content(&entry, args, chub_dir, annotations_dir, source_paths, json)?;

        if json {
            all_outputs.push(result);
        }
    }

    if json && multiple {
        println!("{}", serde_json::to_string_pretty(&all_outputs)?);
    } else if json && all_outputs.len() == 1 {
        println!("{}", serde_json::to_string_pretty(&all_outputs[0])?);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn fetch_entry_content(
    entry: &Entry,
    args: &GetArgs,
    chub_dir: &Path,
    annotations_dir: &Path,
    source_paths: &HashMap<String, Option<PathBuf>>,
    json: bool,
) -> Result<serde_json::Value> {
    let resolved = registry::resolve_doc_path(entry, args.lang.as_deref(), args.version.as_deref());

    let (source_name, doc_path, files) = match resolved {
        ResolvedPath::Resolved {
            source,
            path,
            files,
        } => (source, path, files),
        ResolvedPath::SkillPath {
            source,
            path,
            files,
        } => (source, path, files),
        ResolvedPath::NeedsLanguage { available } => {
            let msg = format!(
                "Language is required for doc '{}'. Available languages: {}. Use --lang <language>",
                entry.id,
                available.join(", ")
            );
            if json {
                return Ok(serde_json::json!({"error": msg, "id": &entry.id}));
            }
            bail!("{}", msg);
        }
        ResolvedPath::LanguageNotAvailable {
            requested,
            available,
        } => {
            let msg = format!(
                "Language '{}' not available for '{}'. Available: {}",
                requested,
                entry.id,
                available.join(", ")
            );
            if json {
                return Ok(serde_json::json!({"error": msg, "id": &entry.id}));
            }
            bail!("{}", msg);
        }
        ResolvedPath::VersionNotFound {
            requested,
            available,
        } => {
            let msg = format!(
                "Version '{}' not found for '{}'. Available versions: {}",
                requested,
                entry.id,
                available.join(", ")
            );
            if json {
                return Ok(serde_json::json!({"error": msg, "id": &entry.id}));
            }
            bail!("{}", msg);
        }
        ResolvedPath::Unresolvable => {
            let msg = format!(
                "Cannot resolve content for '{}'. Run `chub update` to refresh.",
                entry.id
            );
            if json {
                return Ok(serde_json::json!({"error": msg, "id": &entry.id}));
            }
            bail!("{}", msg);
        }
    };

    // Determine source local path for cache lookup
    let source_local_path: Option<&Path> =
        source_paths.get(&source_name).and_then(|p| p.as_deref());

    // Determine the entry file (DOC.md for docs, SKILL.md for skills)
    let entry_file = if entry.entry_type == "skill" {
        "SKILL.md"
    } else {
        "DOC.md"
    };

    // Handle --file flag
    if let Some(ref file_arg) = args.file {
        let requested_files: Vec<&str> = file_arg.split(',').map(|s| s.trim()).collect();
        let mut file_contents = Vec::new();

        for req_file in &requested_files {
            validate_file_path(req_file, &files)?;
            let content = cache::fetch_doc(
                chub_dir,
                source_local_path,
                &source_name,
                &doc_path,
                req_file,
            )?;
            file_contents.push((req_file.to_string(), content));
        }

        if json {
            let file_objs: Vec<serde_json::Value> = file_contents
                .iter()
                .map(|(name, content)| serde_json::json!({"name": name, "content": content}))
                .collect();
            return Ok(serde_json::json!({
                "id": &entry.id,
                "type": &entry.entry_type,
                "files": file_objs,
            }));
        }

        for (name, content) in &file_contents {
            if file_contents.len() > 1 {
                eprintln!("# FILE: {}", name);
            }
            println!("{}", content);
            if file_contents.len() > 1 {
                println!("---");
            }
        }
        return Ok(serde_json::Value::Null);
    }

    // Handle --full flag
    if args.full {
        let all_files =
            cache::fetch_doc_full(chub_dir, source_local_path, &source_name, &doc_path, &files)?;

        if json {
            let file_objs: Vec<serde_json::Value> = all_files
                .iter()
                .map(|(name, content)| serde_json::json!({"name": name, "content": content}))
                .collect();
            return Ok(serde_json::json!({
                "id": &entry.id,
                "type": &entry.entry_type,
                "files": file_objs,
            }));
        }

        if let Some(ref output_path) = args.output {
            let out_dir = Path::new(output_path).join(&entry.id);
            std::fs::create_dir_all(&out_dir)?;
            for (name, content) in &all_files {
                let file_path = out_dir.join(name);
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&file_path, content)?;
            }
            eprintln!("Wrote {} files to {}", all_files.len(), out_dir.display());
            return Ok(serde_json::Value::Null);
        }

        for (i, (name, content)) in all_files.iter().enumerate() {
            if all_files.len() > 1 {
                eprintln!("# FILE: {}", name);
            }
            println!("{}", content);
            if i < all_files.len() - 1 {
                println!("---");
            }
        }
        return Ok(serde_json::Value::Null);
    }

    // Default: fetch entry file only, show additional files hint
    let content = cache::fetch_doc(
        chub_dir,
        source_local_path,
        &source_name,
        &doc_path,
        entry_file,
    )?;

    // Additional files (everything except the main entry file)
    let additional_files: Vec<&String> =
        files.iter().filter(|f| f.as_str() != entry_file).collect();

    // Read annotation
    let annotation = annotations::read_annotation_in(annotations_dir, &entry.id)
        .ok()
        .flatten();

    if json {
        let mut output = serde_json::json!({
            "id": &entry.id,
            "type": &entry.entry_type,
            "content": &content,
        });

        if !additional_files.is_empty() {
            output["additionalFiles"] = serde_json::json!(additional_files);
        }
        if let Some(ref ann) = annotation {
            output["annotation"] = serde_json::json!(ann.note);
        }

        return Ok(output);
    }

    // Human output
    println!("{}", content);

    // Annotation footer
    if let Some(ref ann) = annotation {
        eprintln!("\n--- Agent Note ---");
        eprintln!("{}", ann.note);
    }

    // Additional files hint
    if !additional_files.is_empty() {
        eprintln!("\n--- Additional Files ---");
        for f in &additional_files {
            eprintln!("  {}", f);
        }
        eprintln!("\nFetch with: chub get {} --file <filename>", entry.id);
    }

    // Write to file if requested
    if let Some(ref output_path) = args.output {
        std::fs::write(output_path, &content)?;
        eprintln!("Wrote to {}", output_path);
    }

    Ok(serde_json::Value::Null)
}
