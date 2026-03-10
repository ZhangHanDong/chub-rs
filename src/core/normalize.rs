//! Language alias normalization.
//!
//! Converts short codes to canonical names and back.

use std::collections::HashMap;
use std::sync::LazyLock;

static ALIASES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("js", "javascript");
    m.insert("ts", "typescript");
    m.insert("py", "python");
    m.insert("rb", "ruby");
    m.insert("cs", "csharp");
    m
});

static DISPLAY: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("javascript", "js");
    m.insert("typescript", "ts");
    m.insert("python", "py");
    m.insert("ruby", "rb");
    m.insert("csharp", "cs");
    m
});

/// Normalize a language string: resolve aliases, lowercase.
/// Returns `None` if input is empty/None.
pub fn normalize_language(lang: Option<&str>) -> Option<String> {
    let lang = lang?.trim();
    if lang.is_empty() {
        return None;
    }
    let lower = lang.to_lowercase();
    Some(
        ALIASES
            .get(lower.as_str())
            .map(|s| s.to_string())
            .unwrap_or(lower),
    )
}

/// Convert canonical language name to display abbreviation.
pub fn display_language(lang: &str) -> &str {
    let lower_owned;
    let key = if lang.chars().any(|c| c.is_uppercase()) {
        lower_owned = lang.to_lowercase();
        lower_owned.as_str()
    } else {
        lang
    };
    // Need to handle lifetime - if key is in DISPLAY, return static str
    DISPLAY.get(key).copied().unwrap_or(lang)
}
