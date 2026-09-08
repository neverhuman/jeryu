# SQLite Constraints And Rollback Notes

This split owns one durable relational store: the **dev/e2e fixture store**
(`db/migrations/0001_fixture_model.sql`). It is applied by
`db/seed/build-fixtures.mjs` to an ephemeral SQLite database, seeded with
deterministic rows, and used to emit the derived JSON that the test layer
consumes (`db/fixtures/*.json`). It is never wired into the SPA runtime bundle —
`better-sqlite3` and raw SQL live only under `db/`, and the frontend boundary
test (`apps/web/src/app/__tests__/frontendBoundary.test.ts`) fails the build if
`better-sqlite3` is ever imported from `apps/web/src`.

## 0001 Fixture Model

The migration creates the fixture relational model:

- viewer identity (`viewers`) with its `viewer_permissions` and per-viewer
  `feature_flags`, plus the ordered `recent_repositories` membership;
- `repositories` and their `pull_requests`, each pull request owning
  `pull_request_threads` and `pull_request_checks`;
- `runner_pools` -> `runners` -> `runner_tasks`;
- the viewer's `work_items` queue.

### Foreign key policy

- Every child row that is *owned* by a parent references it with
  `... ON DELETE CASCADE`: `viewer_permissions`, `feature_flags`,
  `recent_repositories`, `pull_requests`, `pull_request_threads`,
  `pull_request_checks`, `runners`, `runner_tasks`, and `work_items` all
  disappear with their owning row. This keeps the fixture graph closed: a seed
  or teardown can drop a `viewer`, `repository`, `runner_pool`, or
  `pull_request` and never leave an orphan.
- Two references are deliberately *not* ownership and use `ON DELETE SET NULL`
  instead: `runner_tasks.repo_id` and `work_items.repo_id`. A repository can be
  removed from the fixture set without deleting fleet history or a viewer's work
  queue; those rows detach (the column is nullable, so `SET NULL` is
  well-defined) rather than cascading away.
- `recent_repositories` cascades from *both* `viewers` and `repositories` and is
  keyed `(viewer_id, repo_id)` with a `UNIQUE (viewer_id, position)` so a
  viewer's recents stay a gap-free ordered list.
- Composite grandchild identity is enforced up the chain: `pull_requests` is
  `UNIQUE (repo_id, number)`, `pull_request_checks` is
  `UNIQUE (pull_request_id, name)`, `runners` is `UNIQUE (pool_id, name)`.

### CHECK policy

- Enum-like columns are constrained to their known wire values:
  `repositories.visibility` (`public`/`internal`/`private`),
  `repositories.repo_role`, `repositories.health`, `pull_requests.state`,
  `pull_requests.passport_status`, `pull_request_checks.status` /
  `conclusion`, `runner_pools.trust_tier`, `runners.status`,
  `runner_tasks.kind` / `state`, and `work_items.kind` / `state` / `priority`.
- Counters are non-negative (`>= 0`); PR/check numbers and slot counts are
  bounded; `jankurai_score` is `NULL` or `BETWEEN 0 AND 100` (matching
  jeryu-core `0007_jankurai_scores`, where `NULL` records an unscoreable audit).
- Identity/text columns that must not be blank use
  `CHECK (length(trim(col)) > 0)`.
- Two cross-column invariants are enforced:
  - a `merged` pull request must carry a non-null `passport_hash` (you cannot
    merge without the passport it was gated on);
  - a `completed` check must have a `conclusion` and a non-completed check must
    not — the check state machine cannot lie about its result.
- Every JSON payload column (`topics_json`, `labels_json`, `output_json`,
  `tags_json`, `payload_json`, `metadata_json`) is guarded by `json_valid(...)`
  so the fixtures can never emit malformed JSON into the wire mocks.

### Covering indexes (db/constraints/0001_fixture_constraints.sql)

SQLite does not auto-index foreign-key child columns. The constraints companion
creates a covering index for each FK child column that is not already the
leftmost column of a primary key or unique index, so every `ON DELETE CASCADE` /
`SET NULL` and every parent -> child read path is index-backed rather than a
full scan. The companion also re-asserts `PRAGMA foreign_keys = ON` for the
seeding connection, because SQLite defaults that pragma **off** per connection
and cascade/detach semantics are otherwise silently skipped.

### Rollback / backfill / lock-safety

- **Rollback** (`db/rollbacks/0001_fixture_model.sql`) drops the tables
  child-first so the cascade FKs never dangle mid-teardown. It is safe to run
  unconditionally: the store is synthetic and fully regenerable, so recovery is
  `npm run fixtures:build`, not a database restore.
- **Backfill**: none. The tables start empty and are populated only by the
  deterministic seed, never by product traffic. There is no
  read-old-write-new window to stage.
- **Lock-safety**: the migration and rollback carry a `-- timeout-guard:` block
  (`lock_timeout = '5s'`, `statement_timeout = '60s'`) mirrored in the
  `.meta.toml`, and the migration is `change_type = "expand"`,
  `safety = "online"`, `transaction = "required"`. Because the only writer is
  the seed script against an ephemeral database, no online traffic can contend
  for the lock; the guard metadata exists so migration analysis has the same
  audit evidence it requires of the durable jeryu-core store.
