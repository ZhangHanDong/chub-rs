# chub-rs Overview

## What chub-rs Is

`chub-rs` is the Rust implementation of the `context-hub` CLI and MCP server model.

The upstream `context-hub` project defines the external behavior and the content format:

- how sources are configured
- how `registry.json` and `search-index.json` are shaped
- how docs and skills are discovered and fetched
- how annotations, update, feedback, and MCP integration behave

`chub-rs` keeps that compatibility model, but reimplements the runtime in Rust.

The practical goal is:

- keep CLI behavior compatible with `context-hub`
- keep content/source format compatible with `context-hub`
- expose the same agent-facing workflows with a smaller, faster Rust binary

So the right mental model is:

- `context-hub` is the behavioral and content-format baseline
- `chub-rs` is the Rust runtime that consumes and serves the same kind of sources

## Core Concepts

### 1. Source

A source is where `chub-rs` reads content from.

There are two source types:

- remote source: `{ name, url }`
- local source: `{ name, path }`

Examples:

```yaml
sources:
  - name: official
    url: https://cdn.aichub.org/v1
  - name: local-dev
    path: /absolute/path/to/dist
```

`chub-rs` should treat both as the same content format. The only difference is transport:

- remote sources are accessed over HTTP
- local sources are accessed from the filesystem

### 2. Registry

Each source publishes a `registry.json`.

This is the catalog of all content in that source:

- docs
- skills
- metadata
- language/version information
- relative content paths
- file lists

At runtime, `chub-rs` loads registries from all configured sources and merges them into a single in-memory view.

### 3. Search Index

Each source may publish a `search-index.json`.

This is a prebuilt BM25 index used for search ranking. If an index is present, `chub-rs` should use it. If not, it can fall back to keyword matching.

### 4. Content Tree

A source also exposes the real markdown files:

- `DOC.md` for docs
- `SKILL.md` for skills
- optional extra files such as `references/*.md`

The registry points to these paths. `get` resolves the correct path and then reads the content.

### 5. Bundle

Remote sources may also expose `bundle.tar.gz`.

This is an offline package of content that `chub update --full` can download and unpack into the local cache.

### 6. Local State

`chub-rs` stores local state under `~/.chub/` by default:

- `config.yaml`
- cached source registries and indexes
- cached content files
- annotations
- client id

This directory can be overridden with `CHUB_DIR`.

## Runtime Architecture

The runtime has three major layers.

### CLI Layer

The CLI layer parses arguments and dispatches commands:

- `search`
- `get`
- `annotate`
- `feedback`
- `update`
- `cache`
- `build`

This layer should stay thin. It should mostly:

- parse args
- load config / merged registry
- call core logic
- format output

### Core Layer

The core layer implements the actual behavior:

- config loading
- source and cache path handling
- registry merging
- BM25 search
- doc/skill resolution
- source fetching
- build pipeline
- annotation persistence
- telemetry and analytics helpers

This is the most important layer for correctness and testability.

### MCP Layer

The MCP layer exposes the same capabilities over stdio JSON-RPC.

It should not reimplement product logic. It should wrap the same runtime behavior into MCP tools/resources, mainly:

- `chub_search`
- `chub_get`
- `chub_list`
- `chub_annotate`
- `chub_feedback`
- `chub://registry`

## How chub-rs Works at Runtime

### Search Flow

When you run `chub search ...`, the high-level flow is:

1. load config
2. load registries from configured sources
3. load search indexes when available
4. merge entries into one view
5. apply source trust filtering, tag filtering, and language filtering
6. run BM25 or fallback search
7. print human output or JSON

### Get Flow

When you run `chub get ...`, the flow is:

1. find the entry by ID
2. if multiple sources expose the same ID, require `source:id`
3. for docs, resolve language and optional version
4. choose the entry-point file:
   - doc -> `DOC.md`
   - skill -> `SKILL.md`
5. fetch content from the best available location
6. append annotation if one exists
7. print or serialize the result

The intended fetch order is:

1. local source path
2. local cache
3. bundled/offline content
4. remote HTTP fetch

### Update Flow

When you run `chub update`, the flow is:

1. load configured sources
2. skip local sources
3. check freshness metadata for remote sources
4. fetch `registry.json`
5. optionally fetch `search-index.json`
6. write cache files and update `meta.json`

When you run `chub update --full`, it also downloads `bundle.tar.gz` and extracts it into the local source cache.

### Annotation Flow

Annotations are local, per-entry notes.

When you run:

```bash
chub annotate some/id "note"
```

the note is saved under the local annotations directory. On the next `get`, the note is appended to the fetched content.

This makes `chub-rs` not just a content reader, but also a lightweight memory layer for repeated agent workflows.

### Feedback and Analytics

There are two separate concepts:

- feedback: explicit thumbs-up / thumbs-down and labels for a specific entry
- analytics: general CLI usage tracking

They are related, but not the same thing.

Feedback is tied to the entry and can include:

- id
- rating
- comment
- language/version/file context
- agent/model context

Analytics is general product telemetry and should never block command success.

## Build Pipeline

The build pipeline is how you create content sources.

This is the author-side flow:

1. create a content directory
2. place `DOC.md` and `SKILL.md` files in it
3. add frontmatter
4. run `chub build <content-dir>`
5. publish the generated `dist/` directory

`chub build` does three main things:

- discovers content
- generates `registry.json` and `search-index.json`
- copies the content tree into the output directory

That output directory is the source artifact.

## How to Create Your Own Source

### Step 1: Create Content

Example structure:

```text
content/
  myteam/
    docs/
      payments/
        javascript/
          DOC.md
          references/
            webhooks.md
    skills/
      deploy-checklist/
        SKILL.md
```

### Step 2: Write Frontmatter

Minimal doc:

```md
---
name: payments
description: "Internal payments API"
metadata:
  languages: "javascript"
  versions: "1.0.0"
  source: "maintainer"
  tags: "api, payments"
  updated-on: "2026-03-10"
---
# Payments API
```

Minimal skill:

```md
---
name: deploy-checklist
description: "Release checklist"
metadata:
  source: "community"
  tags: "skill, deploy"
  updated-on: "2026-03-10"
---
# Deploy Checklist
```

### Step 3: Build

```bash
cargo run -- build ./content -o ./dist
```

Or validate only:

```bash
cargo run -- build ./content --validate-only
```

### Step 4: Use It as a Source

Local source:

```yaml
sources:
  - name: mydocs
    path: /absolute/path/to/dist
```

Remote source:

```yaml
sources:
  - name: mydocs
    url: https://docs.example.com/chub
```

For a remote source, you only need to host the generated `dist/` directory on a static file server.

That means your remote host should expose URLs like:

```text
https://docs.example.com/chub/registry.json
https://docs.example.com/chub/search-index.json
https://docs.example.com/chub/myteam/docs/payments/javascript/DOC.md
```

## Authoring Modes

There are two ways to define source contents.

### Auto-Discovery Mode

You only write `DOC.md` / `SKILL.md` plus frontmatter, and the build step generates the registry for you.

This is the default and simplest approach.

### Author-Supplied Registry Mode

An author directory can provide its own `registry.json`.

When present, the build pipeline uses it instead of auto-discovery.

This is useful when:

- you need exact path/id control
- you already have generated metadata
- you want a more curated source definition

## Compatibility Goal with context-hub

The most important design rule in `chub-rs` is not “be Rusty”; it is “stay compatible.”

That means:

- the source format should stay compatible with `context-hub`
- runtime behavior should match `context-hub` where the upstream implementation is the baseline
- tests should prefer parity with upstream fixtures and observable behavior

This is why `context-hub` matters to `chub-rs`: it is not just inspiration, it is the behavioral contract.

## Practical Summary

If you are using `chub-rs`, the shortest useful mental model is:

- `chub-rs` consumes one or more sources
- each source is just a build artifact directory
- the key artifacts are `registry.json`, `search-index.json`, and markdown content files
- local and remote sources share the same format
- CLI and MCP are two interfaces over the same core behavior
- annotations and feedback add a memory and improvement loop on top of content retrieval

If you are creating your own source, the shortest path is:

1. write `DOC.md` / `SKILL.md`
2. run `chub build`
3. point `sources[].path` or `sources[].url` at the generated output
