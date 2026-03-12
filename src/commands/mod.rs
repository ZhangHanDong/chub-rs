use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::core::annotations;
use crate::core::config;
use crate::core::registry;

pub mod annotate;
pub mod build;
pub mod cache;
pub mod feedback;
pub mod get;
pub mod search;
pub mod update;

#[derive(Parser, Clone)]
#[command(
    name = "chub",
    about = "Context Hub - search and retrieve LLM-optimized docs and skills",
    version,
    long_version = concat!("chub ", env!("CARGO_PKG_VERSION")),
)]
pub struct Cli {
    /// Output as JSON (machine-readable)
    #[arg(long, global = true)]
    pub json: bool,

    /// Print version
    #[arg(long = "cli-version", hide = true)]
    pub cli_version: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Clone)]
pub enum Command {
    /// Search docs and skills (no query lists all)
    Search(search::SearchArgs),
    /// Fetch docs or skills by ID (auto-detects type)
    Get(get::GetArgs),
    /// Attach agent notes to a doc or skill
    Annotate(annotate::AnnotateArgs),
    /// Rate a doc or skill (up/down)
    Feedback(feedback::FeedbackArgs),
    /// Refresh the cached registry index
    Update(update::UpdateArgs),
    /// Manage the local cache
    Cache(cache::CacheArgs),
    /// Build registry.json from a content directory
    Build(build::BuildArgs),
}

/// Print the custom usage screen (mirrors JS CLI `printUsage()`).
pub fn print_usage() {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!(
        r#"chub — Context Hub CLI v{version}
Search and retrieve LLM-optimized docs and skills.

Getting Started

  $ chub update                                # download the registry
  $ chub search                                # list everything available
  $ chub search "stripe"                       # fuzzy search
  $ chub search stripe/payments                # exact id → full detail
  $ chub get stripe/api                        # print doc to terminal
  $ chub get stripe/api -o doc.md              # save to file
  $ chub get openai/chat --lang py             # specific language
  $ chub get pw-community/login-flows          # fetch a skill
  $ chub get openai/chat stripe/api            # fetch multiple

Learn & Improve

  After using a doc, save what you learned so future sessions start smarter:

  $ chub annotate stripe/api "Webhook needs raw body"   # persists across sessions
  $ chub annotate --list                                 # see all saved notes
  $ chub annotate stripe/api --clear                     # remove a note

  Rate docs so authors can improve them (ask the user before sending):

  $ chub feedback stripe/api up                          # worked well
  $ chub feedback stripe/api down --label outdated       # needs updating

Commands

  search [query]              Search docs and skills (no query = list all)
  get <ids...>                 Fetch docs or skills by ID
  annotate [id] [note]        Save a note — appears on future fetches
  feedback <id> <up|down>     Rate a doc (helps authors improve it)
  update                      Refresh the cached registry
  cache status|clear          Manage the local cache
  build <content-dir>        Build registry from content directory

Flags

  --json                 Structured JSON output (for agents and piping)
  --tags <csv>           Filter by tags (e.g. docs, skill, openai, browser)
  --lang <language>      Language variant (required for docs): py | js | ts | rb | cs (or full name)
  --full                 Fetch all files, not just the entry point
  -o, --output <path>    Write content to file or directory"#
    );
}

pub(crate) fn chrono_now_iso() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO-ish format without pulling in chrono crate
    format!("{}Z", now)
}

fn epoch_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Info about a source needed for content resolution.
#[derive(Debug, Clone, Default)]
pub struct SourceInfo {
    pub path: Option<std::path::PathBuf>,
    pub url: Option<String>,
}

/// Load the merged registry from all configured sources.
/// Calls `ensure_registry` first to bootstrap/refresh remote registries.
/// Returns (MergedRegistry, source_info) where source_info maps source name to SourceInfo.
fn load_merged_registry() -> Result<(
    registry::MergedRegistry,
    std::collections::HashMap<String, SourceInfo>,
)> {
    let chub_dir = config::get_chub_dir();
    let cfg = config::load_config_inner(&chub_dir);

    // Bootstrap: ensure at least one registry is available (best-effort)
    crate::core::cache::ensure_registry(&chub_dir, &cfg.sources, cfg.refresh_interval);

    let mut source_data = Vec::new();
    let mut source_info = std::collections::HashMap::new();

    for source in &cfg.sources {
        // Track source info for content resolution
        source_info.insert(
            source.name.clone(),
            SourceInfo {
                path: source.path.clone(),
                url: source.url.clone(),
            },
        );

        // For local sources, read registry directly from source.path (no cache fallback)
        if let Some(ref local_path) = source.path {
            let local_reg_path = local_path.join("registry.json");
            let local_idx_path = local_path.join("search-index.json");
            if let Ok(reg) = registry::load_registry(&local_reg_path) {
                let idx = registry::load_search_index(&local_idx_path).ok();
                source_data.push((source.name.clone(), reg, idx));
            }
            continue;
        }

        // For remote sources only, read from cached dir
        let reg_path = crate::core::cache::get_source_registry_path(&chub_dir, &source.name);
        let idx_path = crate::core::cache::get_source_search_index_path(&chub_dir, &source.name);

        if let Ok(reg) = registry::load_registry(&reg_path) {
            let idx = registry::load_search_index(&idx_path).ok();
            source_data.push((source.name.clone(), reg, idx));
        }
    }

    Ok((registry::merge_registries(&source_data), source_info))
}

/// Dispatch the parsed CLI to the appropriate command handler.
pub fn run(cli: Cli) -> Result<()> {
    // Handle --cli-version alias
    if cli.cli_version {
        println!("chub {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let json = cli.json;
    match cli.command {
        Some(Command::Search(ref args)) => {
            let (merged, _) = load_merged_registry()?;
            search::run(args, &merged, json)
        }
        Some(Command::Get(ref args)) => {
            let (merged, source_info) = load_merged_registry()?;
            let chub_dir = config::get_chub_dir();
            let annotations_dir = annotations::get_annotations_dir();
            get::run(
                args,
                &merged,
                &chub_dir,
                &annotations_dir,
                &source_info,
                json,
            )
        }
        Some(Command::Annotate(ref args)) => {
            let chub_dir = config::get_chub_dir();
            let annotations_dir = annotations::get_annotations_dir_in(&chub_dir);
            let now = chrono_now_iso();
            annotate::run(args, &annotations_dir, &now, json)
        }
        Some(Command::Feedback(ref args)) => {
            let (merged, _) = load_merged_registry()?;
            let chub_dir = config::get_chub_dir();
            let cfg = config::load_config_inner(&chub_dir);
            feedback::run(args, &merged, &chub_dir, &cfg, json)
        }
        Some(Command::Update(ref args)) => {
            let chub_dir = config::get_chub_dir();
            let cfg = config::load_config_inner(&chub_dir);
            let now = epoch_now();
            update::run(args, &chub_dir, &cfg, now, json)
        }
        Some(Command::Cache(ref args)) => {
            let chub_dir = config::get_chub_dir();
            let cfg = config::load_config_inner(&chub_dir);
            cache::run(args, &chub_dir, &cfg, json)
        }
        Some(Command::Build(ref args)) => build::run(args, json),
        None => {
            let _ = json;
            Ok(())
        }
    }
}
