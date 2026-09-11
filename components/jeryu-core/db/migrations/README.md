# Migrations

Core owns the numbered SQLite migrations in this directory. Each stored-shape
change needs rollback, backfill and lock-safety notes. Migration 0013 adds retained
repository creation receipts; see `../constraints.md` before adoption or rollback.
