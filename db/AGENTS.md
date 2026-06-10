# DB Agent Instructions

Owns the durable SQLite schema, migrations, constraints, and migration-analysis
evidence for Jeryu's forge truth.

Allowed edits:
- Add forward-only SQL migrations under `db/migrations/`.
- Update `db/constraints.md` when a migration changes invariants.
- Add rollback/backfill notes with every migration that changes stored shape.

Forbidden edits:
- Do not put application logic, HTTP routing, or web data access in `db/`.
- Do not bypass `jeryu-core`; product code must go through typed forge APIs.
- Do not add destructive migrations without a staged rollback and lock plan.

Proof lane:
- `jankurai migrate . --analyze --out target/jankurai/migration-report.json`
- `cargo test -p jeryu-core --jobs 40 sqlite_open_backfills_pull_request_source_repository`

Migration 0004 notes:
- `pull_requests.source_repository` is provenance metadata only; it must not
  weaken branch-protection review, status, signed-commit, or admin enforcement.
- Keep a `VACUUM INTO` copy before applying the migration to a populated store.
- The open path should guard `PRAGMA table_info(pull_requests)` so repeated
  opens and backfills stay idempotent.

Migration 0005-0007 notes:
- `repositories.family` (0005) is UI grouping data; its seed backfill runs only
  when the column is first added and must never overwrite operator edits.
- `forge_audit_log` (0006) deliberately has NO repository FK and is excluded
  from the full-rewrite persist (`delete_all`/`persist_state`) so delete
  receipts survive both repository deletion and every state rewrite.
- `jankurai_scores` (0007) allows NULL `score` (decision `tool-failed` records
  an unscoreable audit); any new per-repo table MUST be threaded through
  `State`, `load_state`, `persist_state`, and `delete_all`, or the next
  mutation silently wipes it.

Live-readiness note:
- When migrations or constraints change, include this guidance file in the
  changed-fast audit so Jankurai can detect the local DB owner and proof lane.
