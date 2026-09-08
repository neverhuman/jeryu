# DB Agent Guidance

This split does not own ad hoc *product* database writes. It DOES own the
**dev/e2e fixture-seed store** — the durable, typed schema + deterministic seed
behind the test fixtures (`apps/web/e2e/fixtures/mocks.ts`,
`apps/web/e2e/fixtures/data/bootstrap.json`, `apps/web/src/test/mocks.ts`). All
durable schema changes route through `db/migrations/`, `db/constraints/`, and
`db/rollbacks/`, with rollback, backfill, lock-safety, and foreign key / check
constraint notes. The fixtures are single-viewer dev/e2e data with no
tenant-scoped or production truth, so no per-row access isolation applies.

## Allowed edits

- Add forward-only SQL migrations under `db/migrations/` (`NNNN_name.sql` +
  `NNNN_name.meta.toml`), with a paired `db/rollbacks/NNNN_name.sql` and the
  constraint rationale in `db/constraints.md`.
- Extend the deterministic seed and derivations in `db/seed/build-fixtures.mjs`.
- Regenerate `db/fixtures/*.json` via `npm run fixtures:build` (a declared
  generated zone).

## Forbidden edits

- Do NOT import `better-sqlite3` or write raw SQL under `apps/web/src`. The DB
  boundary lives only under `db/`; the frontend boundary test
  (`apps/web/src/app/__tests__/frontendBoundary.test.ts`, whose
  `FORBIDDEN_IMPORTS` includes `better-sqlite3`) fails the build otherwise. The
  round-trip test therefore lives under `db/seed/__tests__/`, not `src/`.
- Do NOT add destructive migrations without a staged rollback and a
  `-- timeout-guard:` block plus `.meta.toml` rollback/backfill/lock fields.
- Do NOT hand-edit `db/fixtures/*.json`; regenerate them from the store.

## Proof lane

- `npm run fixtures:build` — apply migrations + constraints, seed, emit JSON.
- `npm run fixtures:test` — `node --test` round-trip
  (`sqlite_store_round_trips_fixtures`): seeds, reads back, and asserts FK /
  CHECK / cascade / SET NULL / json_valid invariants hold, and that the derived
  bootstrap deep-equals the canonical e2e fixture.
