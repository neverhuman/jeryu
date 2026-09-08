# Architecture

`jeryu-cache` is part of the Jeryu split family.

The public portal is `neverhuman/jeryu`. Release authority remains
`neverhuman/jeryu-deploy`; split member repositories own bounded product
surfaces and consume sibling crates from pinned public Git tags.

## Boundaries

- Profile: `rust-workspace`
- Required check: `jeryu-cache/required`
- Local release source of truth: `agent/boundaries.toml`

## Owned Surface

- `crates/jeryu-cache-core/**`
- `crates/jeryu-cache-service/**`
- `crates/jeryu-cache-cli/**`
- `crates/jeryu-cache-adversary/**`
- `crates/jeryu-cache/**`
- `tests/cache_poisoning_matrix.sh`
- `fixtures/cache-poisoning/**`
- `config/jeryu-cache-policy.toml`
- `policies/cache-laws.toml`
- `examples/cache-key-material.json`
