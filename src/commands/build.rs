use anyhow::{Result, bail};
use clap::Args;
use std::path::Path;

use crate::core::build;

#[derive(Args, Clone)]
pub struct BuildArgs {
    /// Content directory to build from
    pub content_dir: String,

    /// Output directory
    #[arg(short, long)]
    pub output: Option<String>,

    /// Base URL for CDN deployment
    #[arg(long)]
    pub base_url: Option<String>,

    /// Validate without writing output
    #[arg(long)]
    pub validate_only: bool,
}

/// Run the build command.
pub fn run(args: &BuildArgs, json: bool) -> Result<()> {
    let generated = crate::commands::chrono_now_iso();
    run_with_generated(args, json, &generated)
}

/// Run the build command with an injectable timestamp (for testing).
pub fn run_with_generated(args: &BuildArgs, json: bool, generated: &str) -> Result<()> {
    let content_dir = Path::new(&args.content_dir);
    if !content_dir.exists() {
        bail!("Content directory not found: {}", args.content_dir);
    }

    let output_dir_str = args
        .output
        .clone()
        .unwrap_or_else(|| content_dir.join("dist").to_string_lossy().to_string());
    let output_dir = Path::new(&output_dir_str);

    let result = build::build_content_dir(content_dir, generated, args.base_url.as_deref())?;

    // Print warnings to stderr
    if !json {
        for w in &result.warnings {
            eprintln!("Warning: {}", w);
        }
    }

    if args.validate_only {
        let summary = serde_json::json!({
            "docs": result.registry.docs.len(),
            "skills": result.registry.skills.len(),
            "warnings": result.warnings.len(),
            "output": output_dir_str,
        });
        if json {
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            eprintln!(
                "Valid: {} docs, {} skills, {} warnings",
                result.registry.docs.len(),
                result.registry.skills.len(),
                result.warnings.len()
            );
        }
        return Ok(());
    }

    build::write_build_output(&result, content_dir, output_dir)?;

    let summary = serde_json::json!({
        "docs": result.registry.docs.len(),
        "skills": result.registry.skills.len(),
        "warnings": result.warnings.len(),
        "output": output_dir_str,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        eprintln!(
            "Built: {} docs, {} skills -> {}",
            result.registry.docs.len(),
            result.registry.skills.len(),
            output_dir_str
        );
    }

    Ok(())
}
