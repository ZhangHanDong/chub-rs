use std::collections::HashMap;
use std::io::Write;

use chub_rs::commands::SourceInfo;

fn main() {
    // Redirect all stderr output for diagnostics
    let _ = writeln!(
        std::io::stderr(),
        "[chub-mcp] Starting server v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Load config and registry (best-effort)
    let chub_dir = chub_rs::core::config::get_chub_dir();
    let cfg = chub_rs::core::config::load_config_inner(&chub_dir);

    // Bootstrap: ensure at least one registry is available
    chub_rs::core::cache::ensure_registry(&chub_dir, &cfg.sources, cfg.refresh_interval);

    let mut source_data = Vec::new();
    let mut source_info_map: HashMap<String, SourceInfo> = HashMap::new();

    for source in &cfg.sources {
        // Track source info (path + url)
        source_info_map.insert(
            source.name.clone(),
            SourceInfo {
                path: source.path.clone(),
                url: source.url.clone(),
            },
        );

        // For local sources, read registry directly from source.path (no cache fallback)
        if let Some(ref local_path) = source.path {
            let local_reg = local_path.join("registry.json");
            let local_idx = local_path.join("search-index.json");
            if let Ok(reg) = chub_rs::core::registry::load_registry(&local_reg) {
                let idx = chub_rs::core::registry::load_search_index(&local_idx).ok();
                source_data.push((source.name.clone(), reg, idx));
            }
            continue;
        }

        // For remote sources, read from cached dir
        let reg_path = chub_rs::core::cache::get_source_registry_path(&chub_dir, &source.name);
        let idx_path = chub_rs::core::cache::get_source_search_index_path(&chub_dir, &source.name);
        if let Ok(reg) = chub_rs::core::registry::load_registry(&reg_path) {
            let idx = chub_rs::core::registry::load_search_index(&idx_path).ok();
            source_data.push((source.name.clone(), reg, idx));
        }
    }

    // Merge all sources at once to build a single unified search index
    let merged = chub_rs::core::registry::merge_registries(&source_data);
    let entries = merged.entries;
    let search_index = merged.search_index;

    let annotations_dir = chub_rs::core::annotations::get_annotations_dir_in(&chub_dir);
    let telemetry_enabled = chub_rs::commands::feedback::is_telemetry_enabled(&cfg);

    let ctx = chub_rs::mcp::McpContext {
        entries,
        search_index,
        chub_dir,
        annotations_dir,
        source_info: source_info_map,
        telemetry_enabled,
        feedback_endpoint: cfg.telemetry_url.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let _ = writeln!(
        std::io::stderr(),
        "[chub-mcp] Server ready ({} entries loaded)",
        ctx.entries.len()
    );

    chub_rs::mcp::run_stdio(&ctx);
}
