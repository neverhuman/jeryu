# Codegraph Tool-Build Insights

Jeryu’s tool-build scanner is a fast, deterministic codegraph sidecar for
finding repeated normalized code windows that may justify new Jankurai tools,
helpers, codemods, or lint rules. It is built for frequent `~/jmcp` polling:
clients read materialized cluster summaries from SQLite instead of triggering a
fresh repo crawl on every query.

The v1 hot path is Rust-only and cheap:

- walk source files while skipping `.git`, `target`, `node_modules`, generated
  docs, and build artifacts;
- normalize identifiers and literals while preserving control-flow, call,
  macro, member, and operator anchors;
- hash fixed-size normalized line windows with BLAKE3;
- rank repeated windows by occurrence count, file spread, token mass, and total
  duplicated lines;
- persist clusters and feedback under the self-contained codegraph SQLite store.

This is intentionally not an LLM detector. Future analyzers can attach
Tree-sitter AST shingles, rare-token postings, API-call motifs, LSH, and
content-addressed multi-repo indexing to the same cluster shape. AI review
should consume only ranked cluster dossiers.

## MCP

Dedicated tools:

- `jeryu.codegraph.tool_build.status`
- `jeryu.codegraph.tool_build.clusters`
- `jeryu.codegraph.tool_build.feedback`

Ignored clusters are suppressed from normal cluster queries but remain durable
and visible with `include_ignored=true`. Feedback requires a reason.

## CLI

```bash
cargo run -p jeryu-codegraph -- tool-build scan --root . --repo-id local/jeryu --top 50
cargo run -p jeryu-codegraph -- tool-build clusters --top 50
cargo run -p jeryu-codegraph -- tool-build ignore <cluster-id> --reason "fixture boilerplate"
```

## API

- `GET /api/v1/codegraph/tool-build/status`
- `GET /api/v1/codegraph/tool-build/clusters`
- `POST /api/v1/codegraph/tool-build/clusters/{cluster_id}/feedback`

Malformed feedback returns a typed repair body with `purpose`, `reason`,
`common_fixes`, `docs_url`, and `repair_hint`.

Proof lane:

```bash
bash ops/ci/codegraph-tool-build.sh
```
