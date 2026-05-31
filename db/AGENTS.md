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

Live-readiness note:
- When migrations or constraints change, include this guidance file in the
  changed-fast audit so Jankurai can detect the local DB owner and proof lane.
