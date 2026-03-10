use anyhow::{Result, bail};
use clap::Args;
use std::path::Path;

use crate::core::config::Config;
use crate::core::identity;
use crate::core::registry::{MergedRegistry, get_entry};
use crate::core::telemetry::{self, FeedbackPayload, VALID_LABELS};

#[derive(Args, Clone)]
pub struct FeedbackArgs {
    /// Entry ID to rate
    pub id: Option<String>,

    /// Rating: "up" or "down"
    pub rating: Option<String>,

    /// Optional comment
    pub comment: Option<String>,

    /// Explicit type: doc or skill
    #[arg(long = "type")]
    pub entry_type: Option<String>,

    /// Language variant of the doc
    #[arg(long)]
    pub lang: Option<String>,

    /// Version of the doc
    #[arg(long = "doc-version")]
    pub doc_version: Option<String>,

    /// Specific file within the entry
    #[arg(long)]
    pub file: Option<String>,

    /// Feedback label (repeatable)
    #[arg(long = "label")]
    pub labels: Vec<String>,

    /// AI coding tool name
    #[arg(long)]
    pub agent: Option<String>,

    /// LLM model name
    #[arg(long)]
    pub model: Option<String>,

    /// Show telemetry status
    #[arg(long)]
    pub status: bool,
}

/// Run the feedback command.
pub fn run(
    args: &FeedbackArgs,
    merged: &MergedRegistry,
    chub_dir: &Path,
    cfg: &Config,
    json: bool,
) -> Result<()> {
    let telemetry_enabled = is_telemetry_enabled(cfg);

    // --status mode
    if args.status {
        let client_id = identity::get_or_create_client_id(chub_dir).unwrap_or_default();
        let prefix = if client_id.len() >= 8 {
            &client_id[..8]
        } else {
            &client_id
        };

        let status = serde_json::json!({
            "telemetry": telemetry_enabled,
            "endpoint": &cfg.telemetry_url,
            "client_id_prefix": prefix,
            "valid_labels": VALID_LABELS,
        });

        if json {
            println!("{}", serde_json::to_string_pretty(&status)?);
        } else {
            eprintln!(
                "Telemetry: {}",
                if telemetry_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            eprintln!("Endpoint: {}", cfg.telemetry_url);
            eprintln!("Client ID: {}...", prefix);
            eprintln!("Valid labels: {}", VALID_LABELS.join(", "));
        }
        return Ok(());
    }

    let id = match &args.id {
        Some(id) => id,
        None => bail!("Entry ID is required. Usage: chub feedback <id> <up|down>"),
    };

    let rating = match &args.rating {
        Some(r) if r == "up" || r == "down" => r.clone(),
        Some(r) => bail!("Invalid rating '{}'. Use 'up' or 'down'.", r),
        None => bail!("Rating is required. Usage: chub feedback <id> <up|down>"),
    };

    // Resolve entry type
    let entry_type = if let Some(ref t) = args.entry_type {
        t.clone()
    } else {
        let lookup = get_entry(id, &merged.entries);
        match lookup.entry {
            Some(e) => e.entry_type,
            None => "doc".to_string(),
        }
    };

    if !telemetry_enabled {
        let result = serde_json::json!({
            "status": "skipped",
            "reason": "telemetry disabled",
        });
        if json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            eprintln!("Feedback skipped: telemetry is disabled.");
        }
        return Ok(());
    }

    let client_id = identity::get_or_create_client_id(chub_dir)?;

    let payload = FeedbackPayload {
        entry_id: id.clone(),
        entry_type,
        rating,
        comment: args.comment.clone(),
        labels: args.labels.clone(),
        language: args.lang.clone(),
        doc_version: args.doc_version.clone(),
        file: args.file.clone(),
        agent: args.agent.clone(),
        model: args.model.clone(),
        client_id,
    };

    let result = telemetry::send_feedback(&payload, &cfg.telemetry_url, telemetry_enabled)?;

    match result {
        telemetry::FeedbackResult::Sent => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"status": "sent"}))?
                );
            } else {
                eprintln!("Feedback sent. Thank you!");
            }
        }
        telemetry::FeedbackResult::Skipped => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"status": "skipped"}))?
                );
            } else {
                eprintln!("Feedback skipped.");
            }
        }
    }

    Ok(())
}

/// Check if telemetry is enabled (config + env override).
pub fn is_telemetry_enabled(cfg: &Config) -> bool {
    // CHUB_TELEMETRY env var overrides config
    if let Ok(val) = std::env::var("CHUB_TELEMETRY") {
        return val != "0" && val.to_lowercase() != "false";
    }
    cfg.telemetry
}
