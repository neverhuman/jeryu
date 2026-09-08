# Data Boundary

This split repo does not own durable application truth outside the paths documented here. Future SQL schema work must land under `db/migrations/`, with constraints under `db/constraints/`, and must describe rollback, backfill, and lock-safety behavior before release.

Required migration language includes foreign key or check constraint rationale when relational tables are introduced, plus row level security notes when tenant-scoped data appears.
