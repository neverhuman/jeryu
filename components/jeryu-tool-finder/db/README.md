# Finder data boundary

This repository does not own a product database. `db/` exists so the
Jankurai data-truth surface has an explicit empty-root: no application
crate opens SQLite or Postgres, and no RLS is claimed.

Rollback is unused because there are no migrations to apply.
