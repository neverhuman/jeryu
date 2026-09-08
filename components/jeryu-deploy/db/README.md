# Data Boundary

This split repo does not own durable application truth outside the paths documented here. Future SQL schema work must land under `db/migrations/`, with constraints under `db/constraints/`, and must describe rollback, backfill, and lock-safety behavior before release.

Required migration language includes foreign key or check constraint rationale when relational tables are introduced, plus row level security notes when tenant-scoped data appears.

## Release gate

Every release receipt must declare either `data_migration=none` or the exact
ordered migration set and its checksums. A release that changes durable data
must also bind all of the following evidence before deployment:

1. A restorable pre-migration backup and a successful restore rehearsal.
2. The forward command and either a tested rollback command or an explicit
   fix-forward procedure when rollback would destroy accepted writes.
3. Backfill batch limits, maximum lock duration, and compatibility behavior
   while old and new binaries overlap.
4. Deterministic post-migration checks for schema version, constraints, row
   counts, and application reads.

Stop the deployment if the backup cannot be read back, the rollback or
fix-forward path is missing, a lock or backfill exceeds its bound, or any
post-migration check differs from the release receipt. Runtime startup must
not create or rewrite durable schema outside this reviewed migration path.
