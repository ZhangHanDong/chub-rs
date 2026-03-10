//! MCP (Model Context Protocol) server implementation.
//!
//! Exposes Context Hub capabilities via stdio JSON-RPC for use with
//! Claude Code, Cursor, and other MCP-compatible agents.

pub mod handlers;
pub mod protocol;

use std::collections::HashMap;
use std::path::PathBuf;

use crate::core::bm25::SearchIndex;
use crate::core::registry::Entry;

/// Server context holding all state needed by MCP handlers.
pub struct McpContext {
    pub entries: Vec<Entry>,
    pub search_index: Option<SearchIndex>,
    pub chub_dir: PathBuf,
    pub annotations_dir: PathBuf,
    /// source_name -> optional local path
    pub source_paths: HashMap<String, Option<PathBuf>>,
    pub telemetry_enabled: bool,
    pub feedback_endpoint: String,
    pub version: String,
}

impl McpContext {
    /// Process a JSON-RPC request and return an optional response.
    /// Returns None for notifications (no id).
    pub fn dispatch(&self, req: &protocol::JsonRpcRequest) -> Option<protocol::JsonRpcResponse> {
        let id = match &req.id {
            Some(id) => id.clone(),
            None => return None, // Notification — no response
        };

        let params = req.params.clone().unwrap_or(serde_json::json!({}));

        let result = match req.method.as_str() {
            "initialize" => {
                let protocol_version = params
                    .get("protocolVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("2024-11-05");
                serde_json::json!({
                    "protocolVersion": protocol_version,
                    "capabilities": {
                        "tools": {},
                        "resources": {}
                    },
                    "serverInfo": {
                        "name": "chub",
                        "version": self.version
                    }
                })
            }
            "tools/list" => handlers::tool_definitions(),
            "tools/call" => {
                let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let now = crate::commands::chrono_now_iso();
                match tool_name {
                    "chub_search" => handlers::handle_search(self, &args),
                    "chub_get" => handlers::handle_get(self, &args),
                    "chub_list" => handlers::handle_list(self, &args),
                    "chub_annotate" => handlers::handle_annotate(self, &args, &now),
                    "chub_feedback" => handlers::handle_feedback(self, &args),
                    _ => {
                        return Some(protocol::JsonRpcResponse::error(
                            id,
                            -32601,
                            format!("Unknown tool: {}", tool_name),
                        ));
                    }
                }
            }
            "resources/list" => handlers::resource_definitions(),
            "resources/read" => {
                let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                if uri == "chub://registry" {
                    handlers::handle_registry_resource(self)
                } else {
                    return Some(protocol::JsonRpcResponse::error(
                        id,
                        -32602,
                        format!("Unknown resource: {}", uri),
                    ));
                }
            }
            _ => {
                // Unknown method — ignore notifications, error on requests
                return Some(protocol::JsonRpcResponse::error(
                    id,
                    -32601,
                    format!("Method not found: {}", req.method),
                ));
            }
        };

        Some(protocol::JsonRpcResponse::success(id, result))
    }
}

/// Run the MCP server stdio loop.
pub fn run_stdio(ctx: &McpContext) {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: protocol::JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let err = protocol::JsonRpcResponse::error(
                    serde_json::Value::Null,
                    -32700,
                    format!("Parse error: {}", e),
                );
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&err).unwrap_or_default()
                );
                let _ = stdout.flush();
                continue;
            }
        };

        if let Some(response) = ctx.dispatch(&request) {
            let _ = writeln!(
                stdout,
                "{}",
                serde_json::to_string(&response).unwrap_or_default()
            );
            let _ = stdout.flush();
        }
    }
}
