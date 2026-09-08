# DB Agent Guidance

This split does not own ad hoc database writes. Durable schema changes must be routed through `db/migrations/` and `db/constraints/`, with rollback, backfill, lock-safety, foreign key or check constraint, and row level security notes where applicable.
