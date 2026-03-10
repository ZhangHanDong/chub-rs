//! M0 config unit tests.
//! Tests use temp directories — never touch real ~/.chub.

use chub_rs::core::config::{get_chub_dir, load_config_inner};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn config_defaults_match_js() {
    let dir = TempDir::new().expect("create temp dir");
    // No config.yaml → all defaults
    let cfg = load_config_inner(dir.path());

    assert_eq!(cfg.sources.len(), 1);
    assert_eq!(cfg.sources[0].name, "default");
    assert_eq!(
        cfg.sources[0].url.as_deref(),
        Some("https://cdn.aichub.org/v1")
    );
    assert!(cfg.sources[0].path.is_none());
    assert_eq!(cfg.output_dir, ".context");
    assert_eq!(cfg.refresh_interval, 21600);
    assert_eq!(cfg.output_format, "human");
    assert_eq!(cfg.source_filter, "official,maintainer,community");
    assert!(cfg.telemetry);
    assert_eq!(cfg.telemetry_url, "https://api.aichub.org/v1");
}

#[test]
fn config_parses_multi_source_file() {
    let dir = TempDir::new().expect("create temp dir");
    let config_content = r#"
sources:
  - name: community
    url: https://cdn.aichub.org/v1
  - name: internal
    path: /path/to/local/docs
telemetry: false
refresh_interval: 3600
"#;
    std::fs::write(dir.path().join("config.yaml"), config_content).expect("write config");

    let cfg = load_config_inner(dir.path());

    assert_eq!(cfg.sources.len(), 2);
    assert_eq!(cfg.sources[0].name, "community");
    assert_eq!(
        cfg.sources[0].url.as_deref(),
        Some("https://cdn.aichub.org/v1")
    );
    assert!(cfg.sources[0].path.is_none());

    assert_eq!(cfg.sources[1].name, "internal");
    assert!(cfg.sources[1].url.is_none());
    assert_eq!(
        cfg.sources[1].path.as_deref(),
        Some(std::path::Path::new("/path/to/local/docs"))
    );

    assert!(!cfg.telemetry);
    assert_eq!(cfg.refresh_interval, 3600);
}

#[test]
fn config_chub_dir_env_override() {
    // Save and restore to avoid affecting other tests
    let prev = std::env::var("CHUB_DIR").ok();

    // SAFETY: This test runs single-threaded; no other threads read CHUB_DIR.
    unsafe { std::env::set_var("CHUB_DIR", "/tmp/test-chub") };
    assert_eq!(get_chub_dir(), PathBuf::from("/tmp/test-chub"));

    // Restore
    match prev {
        Some(v) => unsafe { std::env::set_var("CHUB_DIR", v) },
        None => unsafe { std::env::remove_var("CHUB_DIR") },
    }
}
