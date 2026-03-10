use anyhow::Result;
use clap::Args;
use std::path::Path;
use std::time::Duration;

use crate::core::cache;
use crate::core::config::Config;

const REGISTRY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Args, Clone)]
pub struct UpdateArgs {
    /// Force re-download even if cache is fresh
    #[arg(long)]
    pub force: bool,

    /// Download the full bundle for offline use
    #[arg(long)]
    pub full: bool,
}

/// Run the update command.
pub fn run(args: &UpdateArgs, chub_dir: &Path, cfg: &Config, now: u64, json: bool) -> Result<()> {
    let mut errors = Vec::new();

    for source in &cfg.sources {
        // Skip local sources
        if source.path.is_some() {
            continue;
        }

        let url = match &source.url {
            Some(u) => u.clone(),
            None => continue,
        };

        // Check freshness (unless --force)
        if !args.force && cache::is_cache_fresh(chub_dir, &source.name, cfg.refresh_interval, now) {
            if !json {
                eprintln!("Source '{}' is up to date.", source.name);
            }
            continue;
        }

        // Fetch registry
        if !json {
            eprintln!("Updating '{}'...", source.name);
        }

        let result = fetch_remote_registry(chub_dir, &source.name, &url, now);
        if let Err(e) = result {
            errors.push(format!("{}: {}", source.name, e));
            continue;
        }

        // --full: download bundle
        if args.full {
            if !json {
                eprintln!("Downloading full bundle for '{}'...", source.name);
            }
            let bundle_result = fetch_full_bundle(chub_dir, &source.name, &url);
            if let Err(e) = bundle_result {
                errors.push(format!("{} bundle: {}", source.name, e));
            }
        }
    }

    if json {
        let output = serde_json::json!({
            "updated": errors.is_empty(),
            "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if !errors.is_empty() {
        for e in &errors {
            eprintln!("Error: {}", e);
        }
    } else {
        eprintln!("Registry updated.");
    }

    Ok(())
}

fn fetch_remote_registry(
    chub_dir: &Path,
    source_name: &str,
    base_url: &str,
    now: u64,
) -> Result<()> {
    cache::fetch_and_save_registry(
        chub_dir,
        source_name,
        || {
            let client = reqwest::blocking::Client::builder()
                .timeout(REGISTRY_TIMEOUT)
                .build()?;

            let reg_url = format!("{}/registry.json", base_url.trim_end_matches('/'));
            let reg_resp = client.get(&reg_url).send()?.error_for_status()?;
            let registry_json = reg_resp.text()?;

            // Try search index (optional)
            let idx_url = format!("{}/search-index.json", base_url.trim_end_matches('/'));
            let idx_json = client.get(&idx_url).send().ok().and_then(|r| {
                if r.status().is_success() {
                    r.text().ok()
                } else {
                    None
                }
            });

            Ok((registry_json, idx_json))
        },
        now,
    )
}

fn fetch_full_bundle(chub_dir: &Path, source_name: &str, base_url: &str) -> Result<()> {
    let bundle_url = std::env::var("CHUB_BUNDLE_URL")
        .unwrap_or_else(|_| format!("{}/bundle.tar.gz", base_url.trim_end_matches('/')));

    let client = reqwest::blocking::Client::builder()
        .timeout(REGISTRY_TIMEOUT)
        .build()?;

    let resp = client.get(&bundle_url).send()?.error_for_status()?;
    let bytes = resp.bytes()?;

    cache::extract_bundle(chub_dir, source_name, &bytes)?;

    Ok(())
}

/// Run the update command with an injected fetcher (for testing).
pub fn run_with_fetcher<F, B>(
    args: &UpdateArgs,
    chub_dir: &Path,
    cfg: &Config,
    now: u64,
    json: bool,
    registry_fetcher: F,
    bundle_fetcher: Option<B>,
) -> Result<()>
where
    F: Fn(&str) -> Result<(String, Option<String>)>,
    B: FnOnce(&str) -> Result<Vec<u8>>,
{
    let mut errors = Vec::new();

    for source in &cfg.sources {
        if source.path.is_some() {
            continue;
        }
        let url = match &source.url {
            Some(u) => u.clone(),
            None => continue,
        };

        if !args.force && cache::is_cache_fresh(chub_dir, &source.name, cfg.refresh_interval, now) {
            continue;
        }

        let result =
            cache::fetch_and_save_registry(chub_dir, &source.name, || registry_fetcher(&url), now);
        if let Err(e) = result {
            errors.push(format!("{}: {}", source.name, e));
        }
    }

    if args.full
        && let Some(bf) = bundle_fetcher
    {
        for source in &cfg.sources {
            if source.path.is_some() {
                continue;
            }
            let url = match &source.url {
                Some(u) => u.clone(),
                None => continue,
            };
            match bf(&url) {
                Ok(data) => {
                    if let Err(e) = cache::extract_bundle(chub_dir, &source.name, &data) {
                        errors.push(format!("{} bundle: {}", source.name, e));
                    }
                }
                Err(e) => {
                    errors.push(format!("{} bundle: {}", source.name, e));
                }
            }
            break; // bundle_fetcher is FnOnce
        }
    }

    if json {
        let output = serde_json::json!({
            "updated": errors.is_empty(),
            "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    }

    Ok(())
}
