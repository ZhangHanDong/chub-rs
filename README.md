# chub-rs

Rust implementation of the [Context Hub](https://github.com/andrewyng/context-hub) CLI — a tool that provides curated, LLM-optimized documentation and skills to AI coding agents.

## Features

- **Search** — BM25 full-text search with tag/language filtering
- **Get** — Retrieve doc or skill content with version and language selection
- **List** — Browse available entries with filtering
- **Build** — Scan content directories, parse frontmatter, generate registry and search index
- **Annotate** — Add per-entry notes that are appended on retrieval
- **Update** — Fetch and cache registries from configured sources
- **MCP Server** — Model Context Protocol server over stdio (JSON-RPC 2.0)
- **Feedback** — Optional telemetry for entry quality ratings

## Installation

```bash
cargo install --path .
```

This produces two binaries:

| Binary | Description |
|--------|-------------|
| `chub` | Main CLI tool |
| `chub-mcp` | MCP server for AI agent integration |

## Usage

```bash
# Search for entries
chub search "openai sdk" --tags api --limit 5

# Get a doc (language required for multi-lang docs)
chub get openai/api-sdk --lang python

# Get a skill
chub get my-skills/deploy

# List all entries
chub list --json

# Build a content directory into registry
chub build ./content --output ./dist

# Annotate an entry
chub annotate openai/api-sdk --note "Use v2 endpoints"

# Update cached registries
chub update
```

### MCP Server

Configure in your AI tool's MCP settings:

```json
{
  "mcpServers": {
    "context-hub": {
      "command": "chub-mcp"
    }
  }
}
```

Exposes 5 tools (`chub_search`, `chub_get`, `chub_list`, `chub_annotate`, `chub_feedback`) and 1 resource (`chub://registry`).

## Configuration

Default config location: `~/.chub/config.yaml` (override with `CHUB_DIR` env var).

```yaml
sources:
  - name: default
    url: https://example.com/registry.json
    path: /optional/local/path

telemetry: false
telemetry_url: https://example.com/feedback
```

## Project Structure

```
src/
├── main.rs              # CLI entry point (clap)
├── bin/chub_mcp.rs      # MCP server binary
├── lib.rs
├── commands/            # CLI command handlers
│   ├── search.rs, get.rs, list.rs, build.rs
│   ├── annotate.rs, update.rs, feedback.rs, cache.rs
│   └── mod.rs
├── core/                # Shared logic
│   ├── config.rs        # Config parsing
│   ├── registry.rs      # Registry loading, merging, lookup
│   ├── bm25.rs          # Full-text search index
│   ├── build.rs         # Content directory → registry
│   ├── cache.rs         # Source caching
│   ├── annotations.rs   # Per-entry annotations
│   ├── frontmatter.rs   # YAML frontmatter parsing
│   └── ...
└── mcp/                 # MCP server
    ├── mod.rs           # McpContext, dispatch, stdio loop
    ├── protocol.rs      # JSON-RPC types
    └── handlers.rs      # Tool and resource handlers
```

## Testing

```bash
cargo test              # Run all 52 tests
cargo clippy --all-targets  # Lint
cargo fmt --check       # Format check
```

## License

MIT
