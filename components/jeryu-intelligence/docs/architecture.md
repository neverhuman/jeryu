# Architecture

`jeryu-intelligence` is part of the Jeryu split family.

The public portal is `neverhuman/jeryu`. Release authority remains
`neverhuman/jeryu-deploy`; split member repositories own bounded product
surfaces and consume sibling crates from pinned public Git tags.

## Boundaries

- Profile: `rust-workspace`
- Required check: `jeryu-intelligence/required`
- Local release source of truth: `agent/boundaries.toml`

## Owned Surface

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
