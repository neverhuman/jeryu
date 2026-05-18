# Boundaries

This is the agent-readable explainer for the boundary manifest in
`agent/boundaries.toml`. The TOML file is the source of truth; this
document explains how to read it and which audit findings reference it.

## Stack

`rust-ts-vite-react-redlinedb-bounded-python` (see `agent/boundaries.toml`
`[stack]`). The release path is Rust-first; Python lives behind
`python/ai-service` and is not the truth surface.

## Rust domain boundary

`[rust] domain_paths` lists files that hold pure decision logic and must
not import side-effecting crates. Today those are:

- `src/decision.rs`
- `src/capsule.rs`
- `src/impact.rs`

`forbidden_domain_imports` is enforced as a no-import list (`std::fs`,
`std::env`, `std::net`, `std::time::SystemTime`, `rand::`, `sqlx::`,
`diesel::`, `reqwest::`, `jansu::`, `tracing::`, `log::`). New domain
files must be added to `domain_paths` rather than relaxing the import
list.

## Database boundary

`[db]` declares `db/` as the durable-truth root, with migrations under
`db/migrations` and constraints under `db/constraints`. Direct DB calls
must live behind a typed adapter; this is what the
`HLT-006-DIRECT-DB-WRONG-LAYER` audit rule checks.

RedlineDB is the only embedded state-store backend allowed in this repo.
Do not enable SQLite features, add SQLite SQLx packages, or use SQLite URLs
as a test fixture, compatibility fallback, or local workaround. If the current
SQLx compatibility surface cannot open a `redline:` URL, the fix belongs in
RedlineDB or the RedlineDB adapter layer, not in a SQLite fallback.

## Queues and contracts

`[queues]` pins event contracts to `contracts/events` and generated
types to `contracts/generated`. Jansu is grandfathered as a brownfield
streaming runtime via `[[streaming_exception]]`; replacements require
generated-contract parity, replay, and operations proof before swap.

## Web boundary

`[typescript] web_paths` covers `apps/web`, `packages/web`,
`packages/ui`. `forbidden_web_imports` blocks direct DB clients in the
browser bundle; data must come over the generated contract surface in
`contracts/generated`.

## Related findings and routes

- `HLT-007-HANDWRITTEN-CONTRACT` (boundary, medium) routes to this file
  via `agent/boundaries.toml`.
- `HLT-006-DIRECT-DB-WRONG-LAYER` (data) routes the fix to the adapter
  layer described above.
- Ownership headers (`//! Owner:`, `//! Proof:`, `//! Invariants:`) on
  each `src/*.rs` file are the per-module boundary contract.
