use anyhow::Result;
use clap::Args;

use crate::core::normalize::display_language;
use crate::core::registry::{self, Entry, MergedRegistry, SearchFilters, get_entry};

#[derive(Args, Clone)]
pub struct SearchArgs {
    /// Search query (omit to list all)
    pub query: Option<String>,

    /// Filter by tags (comma-separated)
    #[arg(long)]
    pub tags: Option<String>,

    /// Filter by language
    #[arg(long)]
    pub lang: Option<String>,

    /// Max results
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

/// Run the search command.
pub fn run(args: &SearchArgs, merged: &MergedRegistry, json: bool) -> Result<()> {
    let filters = SearchFilters {
        tags: args
            .tags
            .as_ref()
            .map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
        lang: args.lang.clone(),
        limit: Some(args.limit),
    };

    match &args.query {
        None => {
            // List all entries
            let entries = registry::apply_filters(&merged.entries, &filters);
            if json {
                let output = serde_json::json!({
                    "results": entries,
                    "total": entries.len(),
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                format_entry_list(&entries);
            }
        }
        Some(query) => {
            // Check for exact ID match first
            let lookup = get_entry(query, &merged.entries);
            if let Some(ref entry) = lookup.entry {
                if json {
                    // Exact match: return the entry itself, not wrapped in results
                    println!("{}", serde_json::to_string_pretty(entry)?);
                } else {
                    format_entry_detail(entry);
                }
                return Ok(());
            }

            if lookup.ambiguous {
                if json {
                    let output = serde_json::json!({
                        "error": "ambiguous",
                        "alternatives": lookup.alternatives,
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                } else {
                    eprintln!("Multiple entries found for '{}'. Use one of:", query);
                    for alt in &lookup.alternatives {
                        eprintln!("  chub search {}", alt);
                    }
                }
                return Ok(());
            }

            // Fuzzy search
            let results = registry::search_entries(query, merged, &filters);
            if json {
                let output = serde_json::json!({
                    "results": results,
                    "total": results.len(),
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else if results.is_empty() {
                eprintln!("No results found for '{}'", query);
            } else {
                format_entry_list(&results);
            }
        }
    }
    Ok(())
}

fn format_entry_list(entries: &[Entry]) {
    for e in entries {
        let type_badge = if e.entry_type == "skill" {
            "[skill]"
        } else {
            "[doc]"
        };

        let langs = match &e.languages {
            Some(ls) => ls
                .iter()
                .map(|l| display_language(&l.language).to_string())
                .collect::<Vec<_>>()
                .join(","),
            None => String::new(),
        };

        let desc = if e.description.len() > 60 {
            format!("{}...", &e.description[..57])
        } else {
            e.description.clone()
        };

        let source = e._source.as_deref().unwrap_or("");
        eprintln!(
            "  {:<30} {:<8} {:<10} {:<10} {}",
            e.id, type_badge, langs, source, desc
        );
    }
}

fn format_entry_detail(entry: &Entry) {
    eprintln!("Name: {}", entry.name);
    eprintln!("ID:   {}", entry.id);
    eprintln!("Type: {}", entry.entry_type);
    if let Some(ref source) = entry._source {
        eprintln!("Source: {}", source);
    }
    eprintln!("Description: {}", entry.description);
    if !entry.tags.is_empty() {
        eprintln!("Tags: {}", entry.tags.join(", "));
    }
    if let Some(ref langs) = entry.languages {
        eprintln!("Languages:");
        for l in langs {
            eprintln!("  {} (recommended: {})", l.language, l.recommended_version);
            for v in &l.versions {
                eprintln!(
                    "    {} - {} KB, updated {}",
                    v.version,
                    v.size / 1024,
                    v.last_updated
                );
            }
        }
    }
}
