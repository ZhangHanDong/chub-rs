use anyhow::{Result, bail};
use clap::Args;
use std::path::Path;

use crate::core::annotations;

#[derive(Args, Clone)]
pub struct AnnotateArgs {
    /// Entry ID to annotate
    pub id: Option<String>,

    /// Annotation text
    pub note: Option<String>,

    /// Remove annotation for this entry
    #[arg(long)]
    pub clear: bool,

    /// List all annotations
    #[arg(long)]
    pub list: bool,
}

/// Run the annotate command.
pub fn run(args: &AnnotateArgs, annotations_dir: &Path, now: &str, json: bool) -> Result<()> {
    // --list mode
    if args.list {
        let anns = annotations::list_annotations_in(annotations_dir)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&anns)?);
        } else {
            if anns.is_empty() {
                eprintln!("No annotations saved.");
            }
            for ann in &anns {
                eprintln!("  {} — {}", ann.id, ann.note);
            }
        }
        return Ok(());
    }

    let id = match &args.id {
        Some(id) => id,
        None => bail!("Entry ID is required. Usage: chub annotate <id> [note]"),
    };

    // --clear mode
    if args.clear {
        let removed = annotations::clear_annotation_in(annotations_dir, id)?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": id,
                    "cleared": removed,
                }))?
            );
        } else if removed {
            eprintln!("Annotation cleared for '{}'", id);
        } else {
            eprintln!("No annotation found for '{}'", id);
        }
        return Ok(());
    }

    // Write or read mode
    match &args.note {
        Some(note) => {
            let ann = annotations::write_annotation_in(annotations_dir, id, note, now)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&ann)?);
            } else {
                eprintln!("Annotation saved for '{}'", id);
            }
        }
        None => {
            // Read mode
            let ann = annotations::read_annotation_in(annotations_dir, id)?;
            match ann {
                Some(ann) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&ann)?);
                    } else {
                        eprintln!("[Agent note — {}]", ann.updated_at);
                        eprintln!("{}", ann.note);
                    }
                }
                None => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "id": id,
                                "annotation": null,
                            }))?
                        );
                    } else {
                        eprintln!("No annotation for '{}'", id);
                    }
                }
            }
        }
    }

    Ok(())
}
