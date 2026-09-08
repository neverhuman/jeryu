# Migrations

No SQL migrations are owned by this split today. Add numbered migrations here only with rollback, backfill, and lock-safety notes in the same change.

The first migration must establish a deterministic numbering convention; every
later migration must be ordered, immutable after release, and checksum-bound in
the release receipt. The same reviewed change must provide:

- a restorable pre-migration backup and restore rehearsal;
- the forward command plus a tested rollback or explicit fix-forward command;
- bounded backfill batches and maximum lock duration;
- compatibility expectations for overlapping binary versions; and
- deterministic post-migration schema, constraint, row-count, and read checks.

Until such a change lands, release evidence must continue to declare
`data_migration=none`. A missing checksum, backup, recovery command, or
verification result is a deployment stop condition.
