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

## 0002 Core Forge Auxiliary Tables

The second migration adds auxiliary rows for users, organizations, teams,
labels, issue comments, review comments, commit statuses, CODEOWNERS contents,
and webhook names. These tables preserve the typed `ForgeCore` resources that
do not need first-class relational columns in 0001.

Constraint policy:
- Users and organizations are unique by login.
- Teams are unique per `(organization, slug)` and cascade with their
  organization.
- Labels, issue comments, review comments, commit statuses, CODEOWNERS, and
  webhook metadata cascade with their repository-owned parent rows.
- JSON payload columns must pass `json_valid`.

Rollback/backfill:
- 0002 is additive. A rollback before production use can drop these auxiliary
  tables after taking a `VACUUM INTO` copy.
- In a populated store, restore from the pre-migration database copy instead of
  deleting auxiliary rows in place.
