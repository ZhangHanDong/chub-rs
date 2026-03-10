//! YAML frontmatter parser for DOC.md / SKILL.md files.

use regex::Regex;
use std::sync::LazyLock;

static FM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^---\r?\n(.*?)\r?\n---\r?\n(.*)$").expect("frontmatter re"));

pub struct Frontmatter {
    pub attributes: serde_yaml::Value,
    pub body: String,
}

/// Parse YAML frontmatter from a markdown string.
/// Returns attributes={} and body=full content if no frontmatter found.
pub fn parse_frontmatter(content: &str) -> Frontmatter {
    match FM_RE.captures(content) {
        Some(caps) => {
            let yaml_str = caps.get(1).map_or("", |m| m.as_str());
            let body = caps.get(2).map_or("", |m| m.as_str());
            let attributes: serde_yaml::Value = serde_yaml::from_str(yaml_str)
                .unwrap_or(serde_yaml::Value::Mapping(Default::default()));
            Frontmatter {
                attributes,
                body: body.to_string(),
            }
        }
        None => Frontmatter {
            attributes: serde_yaml::Value::Mapping(Default::default()),
            body: content.to_string(),
        },
    }
}
