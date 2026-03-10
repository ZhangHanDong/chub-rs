use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::Path;

use crate::core::cache;
use crate::core::config::Config;

#[derive(Args, Clone)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub action: CacheAction,
}

#[derive(Subcommand, Clone)]
pub enum CacheAction {
    /// Show cache information
    Status,
    /// Clear cached data
    Clear {
        /// Skip confirmation
        #[arg(long)]
        force: bool,
    },
}

/// Run the cache command.
pub fn run(args: &CacheArgs, chub_dir: &Path, cfg: &Config, json: bool) -> Result<()> {
    match &args.action {
        CacheAction::Status => {
            let source_info: Vec<(&str, bool)> = cfg
                .sources
                .iter()
                .map(|s| (s.name.as_str(), s.path.is_some()))
                .collect();
            let stats = cache::get_cache_stats(chub_dir, &source_info);

            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                eprintln!("Cache directory: {}", chub_dir.display());
                eprintln!("Exists: {}", stats.exists);
                for s in &stats.sources {
                    eprintln!(
                        "  {} ({}) — registry: {}, files: {}, size: {} bytes",
                        s.name, s.source_type, s.has_registry, s.file_count, s.data_size
                    );
                    if let Some(ts) = s.last_updated {
                        eprintln!("    Last updated: {}", ts);
                    }
                }
            }
            Ok(())
        }
        CacheAction::Clear { force: _ } => {
            cache::clear_cache(chub_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"cleared": true}))?
                );
            } else {
                eprintln!("Cache cleared.");
            }
            Ok(())
        }
    }
}
