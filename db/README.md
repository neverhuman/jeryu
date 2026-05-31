# Jeryu SQLite Truth Store

`db/` is the durable schema boundary for the local forge. `jeryu-core` remains
the truth owner; API, CLI, web, and agent surfaces consume typed application
methods rather than direct SQL.

Migration policy:
- every table has a primary key and owner/repo scoping where applicable;
- cross-resource links use foreign key constraints with `ON DELETE CASCADE`
  only when deleting the parent makes the child meaningless;
- enums and booleans use `CHECK` constraints;
- every migration is idempotency-reviewed and has rollback/backfill notes in
  `db/constraints.md`;
- write paths must execute inside application-owned transactions.

