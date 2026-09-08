# Rollback

Rollback preserves Work data and avoids schema ambiguity.

1. Stop writers to the affected Work database.
2. Export a SQLite backup before changing binaries.
3. Revert the application binary or split-family tag through the normal release
   channel.
4. Do not hand-edit Work rows to repair bridge drift. Use structured bridge
   evidence from `jeryu-deploy` to replay or repair Work links.
5. Re-run `rtk just migration-drift` before re-enabling writers.
