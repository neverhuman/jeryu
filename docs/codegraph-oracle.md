# Codegraph Oracle

`jeryu-codegraph` owns an auxiliary SQLite store for workspace graph indexing.
The store is scoped by repository and commit, and refreshes append a new index
receipt plus an outbox event without deleting other repositories or historical
index receipts.

## Storage Rules

- SQLite is the only v1 storage backend.
- The schema is versioned and migrates on open.
- Legacy unversioned databases are backed up with a `.pre-schema-v1-*.bak`
  marker before migration, and a migration receipt is written next to the
  database file.
- Cache hits reuse the latest matching receipt and do not write a new outbox
  event.

## Rollback Notes

- The current database lives under `~/.jeryu/codegraph.sqlite` by default.
- If a migration misbehaves, restore the backup file written beside the
  database and remove the generated `*.migration.json` receipt.
- The store is self-contained; rollback does not require touching
  `db/migrations/` or the shared `jeryu-core` storage tables.
