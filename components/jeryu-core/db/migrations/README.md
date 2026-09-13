# Migrations

Core owns the numbered SQLite migrations in this directory. Each stored-shape
change needs rollback, backfill and lock-safety notes. Two independently named
0013 migrations are preserved from their original histories: repository creation
receipts and repository mutation restrictions. The owning open path applies both
idempotently; neither filename supersedes the other. See `../constraints.md`
before adoption or rollback.
