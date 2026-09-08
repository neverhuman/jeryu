# Data Boundary

This split repo does not own durable *product* truth outside the paths
documented here. It does own one durable relational store: the **dev/e2e
fixture-seed store**, which consolidates the test fixtures that previously lived
as hand-maintained literals (`apps/web/e2e/fixtures/mocks.ts`,
`apps/web/e2e/fixtures/data/bootstrap.json`, `apps/web/src/test/mocks.ts`).

SQL schema work lands under `db/migrations/`, with constraints under
`db/constraints/` and reversals under `db/rollbacks/`, and must describe
rollback, backfill, and lock-safety behavior before release. Required migration
language includes foreign key / check constraint rationale when relational
tables are introduced (see `db/constraints.md`).

## Layout

- `migrations/NNNN_name.sql` (+ `.meta.toml`) — forward-only schema. `0001`
  defines the fixture model: viewer + permissions + feature flags,
  repositories -> pull_requests -> threads/checks, runner pools -> runners ->
  tasks, and work items — with real `CHECK`, `FOREIGN KEY ... ON DELETE
  CASCADE`/`SET NULL`, and `json_valid()` constraints.
- `constraints/NNNN_name.sql` — the FK-covering-index / integrity companion the
  seed applies after the migration. `constraints.md` carries the FK/CHECK
  rationale and the rollback/backfill/lock-safety prose.
- `rollbacks/NNNN_name.sql` (+ `.meta.toml`) — the reversal, dropping tables
  child-first.
- `seed/build-fixtures.mjs` — Node ESM builder (uses `better-sqlite3`). Applies
  the migrations + constraints to an ephemeral SQLite database, inserts the
  deterministic seed rows, and emits the derived fixture JSON. It lives OUTSIDE
  `apps/web/src` so `better-sqlite3` and raw SQL never enter the SPA bundle.
- `seed/__tests__/` — the `sqlite_store_round_trips_fixtures` round-trip test
  (`node --test`), kept outside `apps/web/src` so it may import
  `better-sqlite3` without tripping the frontend boundary test.
- `fixtures/` — the generated JSON the test layer consumes (declared in
  `agent/generated-zones.toml`; never hand-edited). `fixtures/bootstrap.json`
  reproduces the canonical e2e bootstrap fixture from the store.

## Build / test

- `npm run fixtures:build` — rebuild the store and re-emit `db/fixtures/*.json`.
- `npm run fixtures:test` — run the round-trip proof.
