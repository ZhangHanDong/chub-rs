use serde::Serialize;
use std::io::Write;

/// Print structured output: JSON to stdout if `json` is true,
/// otherwise invoke the human formatter closure.
pub fn output<T: Serialize, F: FnOnce(&T)>(data: &T, human_formatter: F, json: bool) {
    if json {
        if let Ok(s) = serde_json::to_string_pretty(data) {
            println!("{s}");
        }
    } else {
        human_formatter(data);
    }
}

/// Print an error message respecting `--json` mode.
/// In JSON mode, writes `{"error": "..."}` to stdout.
/// In human mode, writes `Error: ...` to stderr.
pub fn print_error(msg: &str, json: bool) {
    if json {
        if let Ok(s) = serde_json::to_string_pretty(&serde_json::json!({ "error": msg })) {
            println!("{s}");
        }
    } else {
        let _ = writeln!(std::io::stderr(), "Error: {msg}");
    }
}

/// Print an informational message to stderr.
/// Used when stdout is reserved for content (e.g. `-o` flag).
pub fn info(msg: &str) {
    let _ = writeln!(std::io::stderr(), "{msg}");
}
