use anyhow::{Result, bail};
use clap::Args;
use std::collections::HashMap;
use std::path::Path;

use crate::commands::SourceInfo;
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

/// Intermediate result from fetching one entry.
struct FetchedEntry {
    id: String,
    entry_type: String,
    /// Single-file content (default mode or --file with 1 file).
    content: Option<String>,
    /// Multi-file content (--full or --file with multiple files).
    files: Option<Vec<(String, String)>>,
    /// Additional reference files beyond the entry file.
    additional_files: Vec<String>,
    /// Optional annotation note.
    annotation: Option<String>,
}

/// Run the get command.
pub fn run(
    args: &GetArgs,
    merged: &MergedRegistry,
    chub_dir: &Path,
    annotations_dir: &Path,
    source_info: &HashMap<String, SourceInfo>,
    json: bool,
) -> Result<()> {
    if args.ids.is_empty() {
        bail!("No entry IDs specified. Usage: chub get <id> [id2 ...]");
    }

    let multiple = args.ids.len() > 1;

    // Phase 1: Collect all results
    let mut results: Vec<FetchedEntry> = Vec::new();
    let mut json_errors: Vec<serde_json::Value> = Vec::new();

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
                json_errors.push(serde_json::json!({"error": msg, "id": id}));
                continue;
            }
            bail!("{}", msg);
        }

        let entry = match lookup.entry {
            Some(e) => e,
            None => {
                let msg = format!("No doc or skill found with id '{}'", id);
                if json {
                    json_errors.push(serde_json::json!({"error": msg, "id": id}));
                    continue;
                }
                bail!("{}", msg);
            }
        };

        match fetch_entry_data(&entry, args, chub_dir, annotations_dir, source_info) {
            Ok(fetched) => results.push(fetched),
            Err(e) => {
                if json {
                    json_errors.push(
                        serde_json::json!({"error": e.to_string(), "id": entry.id}),
                    );
                } else {
                    return Err(e);
                }
            }
        }
    }

    // Phase 2: Output
    if let Some(ref output_path) = args.output {
        if results.is_empty() {
            // All entries failed — don't create/truncate the output file.
            // Only emit JSON errors if in JSON mode.
            if json && !json_errors.is_empty() {
                if json_errors.len() == 1 {
                    println!("{}", serde_json::to_string_pretty(&json_errors[0])?);
                } else {
                    println!("{}", serde_json::to_string_pretty(&json_errors)?);
                }
            }
        } else {
            output_to_file(args, &results, output_path, multiple, json, &json_errors)?;
        }
    } else {
        output_to_stdout(&results, multiple, json, &json_errors)?;
    }

    Ok(())
}

/// Fetch entry data without performing any output.
fn fetch_entry_data(
    entry: &Entry,
    args: &GetArgs,
    chub_dir: &Path,
    annotations_dir: &Path,
    source_info: &HashMap<String, SourceInfo>,
) -> Result<FetchedEntry> {
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
            bail!(
                "Language is required for doc '{}'. Available languages: {}. Use --lang <language>",
                entry.id,
                available.join(", ")
            );
        }
        ResolvedPath::LanguageNotAvailable {
            requested,
            available,
        } => {
            bail!(
                "Language '{}' not available for '{}'. Available: {}",
                requested,
                entry.id,
                available.join(", ")
            );
        }
        ResolvedPath::VersionNotFound {
            requested,
            available,
        } => {
            bail!(
                "Version '{}' not found for '{}'. Available versions: {}",
                requested,
                entry.id,
                available.join(", ")
            );
        }
        ResolvedPath::Unresolvable => {
            bail!(
                "Cannot resolve content for '{}'. Run `chub update` to refresh.",
                entry.id
            );
        }
    };

    let info = source_info.get(&source_name);
    let source_local_path: Option<&Path> = info.and_then(|i| i.path.as_deref());
    let source_url: Option<&str> = info.and_then(|i| i.url.as_deref());

    let entry_file = if entry.entry_type == "skill" {
        "SKILL.md"
    } else {
        "DOC.md"
    };

    // --file mode
    if let Some(ref file_arg) = args.file {
        let requested_files: Vec<&str> = file_arg.split(',').map(|s| s.trim()).collect();
        let mut file_contents = Vec::new();

        for req_file in &requested_files {
            validate_file_path(req_file, &files)?;
            let content = cache::fetch_doc(
                chub_dir,
                source_local_path,
                source_url,
                &source_name,
                &doc_path,
                req_file,
            )?;
            file_contents.push((req_file.to_string(), content));
        }

        if file_contents.len() == 1 {
            let (_, content) = file_contents.into_iter().next().unwrap();
            return Ok(FetchedEntry {
                id: entry.id.clone(),
                entry_type: entry.entry_type.clone(),
                content: Some(content),
                files: None,
                additional_files: vec![],
                annotation: None,
            });
        }

        return Ok(FetchedEntry {
            id: entry.id.clone(),
            entry_type: entry.entry_type.clone(),
            content: None,
            files: Some(file_contents),
            additional_files: vec![],
            annotation: None,
        });
    }

    // --full mode
    if args.full {
        let all_files = cache::fetch_doc_full(
            chub_dir,
            source_local_path,
            source_url,
            &source_name,
            &doc_path,
            &files,
        )?;

        return Ok(FetchedEntry {
            id: entry.id.clone(),
            entry_type: entry.entry_type.clone(),
            content: None,
            files: Some(all_files),
            additional_files: vec![],
            annotation: None,
        });
    }

    // Default: single entry file
    let content = cache::fetch_doc(
        chub_dir,
        source_local_path,
        source_url,
        &source_name,
        &doc_path,
        entry_file,
    )?;

    let additional_files: Vec<String> = files
        .into_iter()
        .filter(|f| f.as_str() != entry_file)
        .collect();

    let annotation = annotations::read_annotation_in(annotations_dir, &entry.id)
        .ok()
        .flatten()
        .map(|a| a.note);

    Ok(FetchedEntry {
        id: entry.id.clone(),
        entry_type: entry.entry_type.clone(),
        content: Some(content),
        files: None,
        additional_files,
        annotation,
    })
}

/// Handle -o/--output: write results to file(s).
/// Matches upstream JS behavior:
/// - --full: single entry writes directly to output dir; multi entry nests under <output>/<id>/
/// - non-full, non-dir output: combine all content with \n\n---\n\n separator
/// - non-full, dir output (ends with /): write each to <dir>/<id>.md
fn output_to_file(
    args: &GetArgs,
    results: &[FetchedEntry],
    output_path: &str,
    multiple: bool,
    json: bool,
    json_errors: &[serde_json::Value],
) -> Result<()> {
    if args.full {
        for r in results {
            if let Some(ref files) = r.files {
                // Single entry: write directly to output dir
                // Multi entry: nest under <output>/<id>/
                let base_dir = if multiple {
                    Path::new(output_path).join(&r.id)
                } else {
                    Path::new(output_path).to_path_buf()
                };
                std::fs::create_dir_all(&base_dir)?;
                for (name, content) in files {
                    let file_path = base_dir.join(name);
                    if let Some(parent) = file_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&file_path, content)?;
                }
                eprintln!("Wrote {} files to {}", files.len(), base_dir.display());
            } else if let Some(ref content) = r.content {
                let out = Path::new(output_path).join(format!("{}.md", r.id));
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&out, content)?;
                eprintln!("Wrote to {}", out.display());
            }
        }
    } else {
        let is_dir = output_path.ends_with('/');
        if is_dir && multiple {
            // Directory mode: write each entry to <dir>/<id>.md
            std::fs::create_dir_all(output_path)?;
            for r in results {
                if let Some(ref content) = r.content {
                    let out = Path::new(output_path).join(format!("{}.md", r.id));
                    if let Some(parent) = out.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&out, content)?;
                    eprintln!("Wrote to {}", out.display());
                }
            }
        } else {
            // File mode: combine all content, write once
            let out = if is_dir {
                Path::new(output_path)
                    .join(format!("{}.md", results.first().map_or("output", |r| &r.id)))
            } else {
                Path::new(output_path).to_path_buf()
            };
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let combined: String = results
                .iter()
                .filter_map(|r| r.content.as_deref())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            std::fs::write(&out, &combined)?;
            eprintln!("Wrote to {}", out.display());
        }
    }

    if json {
        let mut all_json: Vec<serde_json::Value> = json_errors.to_vec();
        all_json.extend(results.iter().map(|r| {
            serde_json::json!({
                "id": &r.id,
                "type": &r.entry_type,
                "path": output_path,
            })
        }));
        if all_json.len() == 1 {
            println!("{}", serde_json::to_string_pretty(&all_json[0])?);
        } else {
            println!("{}", serde_json::to_string_pretty(&all_json)?);
        }
    }

    Ok(())
}

/// Handle stdout output (no -o flag).
fn output_to_stdout(
    results: &[FetchedEntry],
    multiple: bool,
    json: bool,
    json_errors: &[serde_json::Value],
) -> Result<()> {
    if json {
        let mut all_json: Vec<serde_json::Value> = json_errors.to_vec();
        for r in results {
            if let Some(ref files) = r.files {
                let file_objs: Vec<serde_json::Value> = files
                    .iter()
                    .map(|(name, content)| serde_json::json!({"name": name, "content": content}))
                    .collect();
                all_json.push(serde_json::json!({
                    "id": &r.id,
                    "type": &r.entry_type,
                    "files": file_objs,
                }));
            } else {
                let mut output = serde_json::json!({
                    "id": &r.id,
                    "type": &r.entry_type,
                    "content": &r.content,
                });
                if !r.additional_files.is_empty() {
                    output["additionalFiles"] = serde_json::json!(&r.additional_files);
                }
                if let Some(ref ann) = r.annotation {
                    output["annotation"] = serde_json::json!(ann);
                }
                all_json.push(output);
            }
        }

        if multiple || all_json.len() > 1 {
            println!("{}", serde_json::to_string_pretty(&all_json)?);
        } else if all_json.len() == 1 {
            println!("{}", serde_json::to_string_pretty(&all_json[0])?);
        }
        return Ok(());
    }

    // Human output
    if results.len() == 1 && results[0].files.is_none() {
        let r = &results[0];
        if let Some(ref content) = r.content {
            println!("{}", content);
        }
        if let Some(ref ann) = r.annotation {
            eprintln!("\n--- Agent Note ---");
            eprintln!("{}", ann);
        }
        if !r.additional_files.is_empty() {
            eprintln!("\n--- Additional Files ---");
            for f in &r.additional_files {
                eprintln!("  {}", f);
            }
            eprintln!("\nFetch with: chub get {} --file <filename>", r.id);
        }
    } else {
        let parts: Vec<String> = results
            .iter()
            .flat_map(|r| {
                if let Some(ref files) = r.files {
                    files
                        .iter()
                        .map(|(name, content)| format!("# FILE: {}\n\n{}", name, content))
                        .collect::<Vec<_>>()
                } else if let Some(ref content) = r.content {
                    vec![content.clone()]
                } else {
                    vec![]
                }
            })
            .collect();
        let combined = parts.join("\n\n---\n\n");
        println!("{}", combined);
    }

    Ok(())
}
