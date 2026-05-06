# db

Cell guidance for the `db/` reference-profile path. See `agent/boundaries.toml` (`[db] root_paths = ["db"]`), `agent/owner-map.json` (`db/` → `data`), and `db/README.md`.

## Owns

Durable schema, migrations, and constraint declarations — the source of truth for persistence shape.

**Honest status: scaffold.** jeryu is a single-binary CLI; durable state is a local SQLite (or in-memory) cache whose live schema is created at startup inside `src/state.rs`, `src/epoch.rs`, and `src/cache_brain.rs` (see `agent/owner-map.json`: `src/state.rs` → `data`). Today the directory holds two markers:

- `db/migrations/0001_inline_schema.sql` — points at the in-code schema.
- `db/constraints/0001_inline_constraints.sql` — points at inline `PRIMARY KEY` / `UNIQUE` / `NOT NULL` declarations.

These exist so the `[db]` boundary lane routes to real artifacts. When the schema is extracted into discrete files, replace the markers and continue numbering forward.

## Forbidden

- Application-owned remote databases or transactional Postgres surfaces (none today).
- Duplicating inline schema here without a migration plan — markers warn against silent drift.
- Domain modules using `sqlx::` directly (forbidden by `boundaries.toml`); persistence access must go through `crates/adapters/cache-brain/` or `src/cache_brain.rs` / `src/state.rs`.

## Proof lane

State-change lane (`proof-lanes.toml`):

```
cargo check -p jeryu --message-format=json
cargo nextest run -p jeryu --lib
cargo test -p jeryu --tests -- --test-threads=1   # required_for = ["state-change"]
```

When a real Postgres surface lands, also run `just postgres-state-proof`.
