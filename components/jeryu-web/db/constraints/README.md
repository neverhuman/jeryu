# Constraints

The relational invariants (CHECK, FOREIGN KEY ... ON DELETE CASCADE / SET NULL,
json_valid) are declared inline in the `db/migrations/` DDL because SQLite only
accepts table constraints at `CREATE TABLE` time. This directory holds the
constraint *catalog and enforcement companion* the seed applies after each
migration; `db/constraints.md` carries the check-constraint, foreign-key, and
rollback/backfill/lock-safety rationale. These are single-viewer dev/e2e
fixtures with no tenant-scoped or production data, so per-row access isolation
does not apply.

- `0001_fixture_constraints.sql` — re-asserts `PRAGMA foreign_keys = ON` and
  creates the covering indexes SQLite does not auto-create for foreign-key child
  columns, so every cascade/detach and parent -> child read is index-backed.
