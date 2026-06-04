# Codegraph Oracle

`jeryu-codegraph` is Jeryu's hosted code oracle for repo/ref impact context.
It indexes a materialized repository checkout at a resolved commit, stores the
refresh in its own SQLite database, and returns a provenance-bearing
`CodeGraphImpactPack`.

## Contract

- REST: `POST /api/v1/repos/{id}/codegraph/query`
- MCP: `jeryu.codegraph.query`
- CLI: `jeryu-codegraph query --root . --changed <path> --json`

The request accepts a ref, repo-relative changed paths, optional intent/question
text, and an optional token budget. The response includes the resolved commit,
changed and affected Rust crates, affected public symbols, must-read files,
should-read files, proof lanes, suggested commands, excluded heuristic matches,
graph stats, residual risk, provenance, and an index receipt.

## V1 Scope

Rust/Cargo graph reachability is the authoritative analyzer in v1. Governance
metadata is ingested from these files when present:

- `AGENTS.md`
- `agent/owner-map.json`
- `agent/test-map.json`
- `agent/generated-zones.toml`
- `agent/proof-lanes.toml`

Generated-zone and owner/proof-lane metadata stays attached to impacted files so
agents can see editability and proof obligations. Lexical intent/question
matches are reported only in `excluded_files`; they are never promoted to
`must_read_files`.

## Proof

- `cargo test -p jeryu-codegraph --jobs 40`
- `cargo test -p jeryu-api --features web --jobs 40 codegraph`
- `cargo test -p jeryu-mcp --jobs 40`
- `jankurai diff-audit --base-ref origin/main .`
