use serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_CDN_URL: &str = "https://cdn.aichub.org/v1";
const DEFAULT_TELEMETRY_URL: &str = "https://api.aichub.org/v1";
const DEFAULT_REFRESH_INTERVAL: u64 = 21600;

#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    /// Remote URL (mutually exclusive with `path`).
    pub url: Option<String>,
    /// Local filesystem path.
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub sources: Vec<Source>,
    pub output_dir: String,
    pub refresh_interval: u64,
    pub output_format: String,
    pub source_filter: String,
    pub telemetry: bool,
    pub telemetry_url: String,
}

// --- Raw YAML deserialization types ---

#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    sources: Option<Vec<RawSource>>,
    cdn_url: Option<String>,
    output_dir: Option<String>,
    refresh_interval: Option<u64>,
    output_format: Option<String>,
    source: Option<String>,
    telemetry: Option<bool>,
    telemetry_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    name: String,
    url: Option<String>,
    path: Option<String>,
}

/// Return the chub data directory. Respects `CHUB_DIR` env var.
pub fn get_chub_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CHUB_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chub")
}

/// Load config from `<chub_dir>/config.yaml`.
///
/// This is a pure function (reads from a specific path) so tests can point
/// `CHUB_DIR` at a temp directory. For the global cached version, callers
/// should wrap this in a `OnceLock` at the call site.
pub fn load_config_inner(chub_dir: &std::path::Path) -> Config {
    let config_path = chub_dir.join("config.yaml");
    let raw: RawConfig = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_yaml::from_str(&s).ok())
        .unwrap_or_default();

    let sources = if let Some(raw_sources) = raw.sources {
        raw_sources
            .into_iter()
            .map(|s| Source {
                name: s.name,
                url: s.url,
                path: s.path.map(PathBuf::from),
            })
            .collect()
    } else {
        let url = std::env::var("CHUB_BUNDLE_URL")
            .ok()
            .or(raw.cdn_url)
            .unwrap_or_else(|| DEFAULT_CDN_URL.to_string());
        vec![Source {
            name: "default".to_string(),
            url: Some(url),
            path: None,
        }]
    };

    Config {
        sources,
        output_dir: raw.output_dir.unwrap_or_else(|| ".context".to_string()),
        refresh_interval: raw.refresh_interval.unwrap_or(DEFAULT_REFRESH_INTERVAL),
        output_format: raw.output_format.unwrap_or_else(|| "human".to_string()),
        source_filter: raw
            .source
            .unwrap_or_else(|| "official,maintainer,community".to_string()),
        telemetry: raw.telemetry.unwrap_or(true),
        telemetry_url: raw
            .telemetry_url
            .unwrap_or_else(|| DEFAULT_TELEMETRY_URL.to_string()),
    }
}
