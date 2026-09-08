# Architecture

Work Tracker is a small Rust-owned persistence and contract surface.

- `crates/jeryu-jira/src/contracts.rs` owns the serializable Work API model and
  `ts-rs` export list.
- `crates/jeryu-jira/src/store/` owns SQLite persistence, request validation,
  row mapping, and repair hints.
- `db/migrations/` owns the canonical schema applied at runtime.
- `contracts/generated/` is generated TypeScript for consumers such as
  `jeryu-web`.
- `ops/ci/` owns local and hosted proof lanes.

The user-facing product name is Work or Work Tracker. Repository and crate names
may contain `jeryu-jira`; product routes, UI copy, and docs should not expose
Jira branding.
