# SQLite Constraints And Rollback Notes

## 0001 Core Forge Tables

The initial migration creates durable rows for repositories, issues, pull
requests, reviews, check runs, branch protection, webhooks, webhook deliveries,
and per-repository counters.

Constraint policy:
- `repositories.full_name` and `(owner, name)` are unique.
- Issues and pull requests are unique per `(repo_id, number)`.
- Reviews, check runs, branch protection rules, webhooks, and deliveries carry
  foreign key references back to their repository.
- State fields use `CHECK` constraints for known wire values.
- Counters use `CHECK (issue_next > 0)` and `CHECK (pull_next > 0)`.

Rollback/backfill:
- Before applying a shape-changing migration, take a copy with SQLite
  `VACUUM INTO`.
- Backfills must run inside a transaction and record row counts in the migration
  report.
- Rollback for 0001 is dropping the empty schema before first production use; in
  a populated store, restore from the pre-migration copy instead of destructive
  down-SQL.
- Long-running backfills should acquire the application migration lock before
  writes and release it only after constraints validate.

