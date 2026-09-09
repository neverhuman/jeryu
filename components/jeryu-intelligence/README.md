# jeryu-intelligence

Codegraph, RustJet, MCP intelligence, review, and autonomy analysis.

This repository was seeded from Jeryu source commit `cbecf7caa0e932c76a341b2521e66e911233860d` by
`ops/split/materialize.py`. It is part of the seven-repo Jeryu split family and keeps source
paths stable where practical so ownership remains auditable.

## Owned Cargo Packages

- `crates/jeryu-codegraph`
- `crates/jeryu-rustjet`
- `crates/jeryu-rustjet-cli`
- `crates/jeryu-mcp`
- `crates/jeryu-review`
- `crates/jeryu-autonomy`

## Source Coverage

- `crates/jeryu-codegraph/**`
- `crates/jeryu-rustjet/**`
- `crates/jeryu-rustjet-cli/**`
- `crates/jeryu-mcp/**`
- `crates/jeryu-review/**`
- `crates/jeryu-autonomy/**`
- `fixtures/rust-small/**`
- `docs/codegraph-oracle.md`
- `docs/codegraph-tool-build.md`
- `ops/ci/codegraph-oracle.sh`
- `ops/ci/codegraph-tool-build.sh`

## Local Commands

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`

## Governed auditor

CI invokes only the receipt-verified `/home/ubuntu/.jeryu/bin/jankurai` identity
rendered by `jeryu-tool`. The 1.6.11 auditor cutover is CI authority only; it
does not change this repository's product version, release tag, or artifacts.
