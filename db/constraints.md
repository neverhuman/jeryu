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

## 0003 Core Forge README Rows

The third migration persists one canonical README markdown row per repository
so the local publish flow can round-trip README updates through the typed
`ForgeCore` boundary instead of mutating the tracked file directly.

Constraint policy:
- `repository_readmes.repo_id` is the primary key and cascades with the owning
  repository row.
- `repository_readmes.contents` stores the canonical markdown source text and
  must remain as raw UTF-8 text.
- Repositories without a stored README continue to synthesize the local
  fallback README at read time until a publish helper writes the managed block.

Rollback/backfill:
- 0003 is additive. Before applying it to a populated store, take a `VACUUM
  INTO` copy and keep that pre-migration database as the rollback target.
- No backfill is required because existing repositories keep their synthesized
  README until the first local publish writes a persisted row.
- If a rollback is needed after content has been published, restore the
  pre-migration database copy rather than deleting `repository_readmes` rows in
  place.

## 0004 Pull Request Source Repository

The fourth migration adds `pull_requests.source_repository` so pull requests
can record the originating repository full name for fork and trust checks.

Constraint policy:
- `source_repository` is stored as `TEXT NOT NULL` with a default empty string
  during the schema change, then backfilled to the owning repository full name.
- New PRs default the field to the base repository full name unless an
  explicit non-empty source repository is supplied.
- The SQLite open path must check `PRAGMA table_info(pull_requests)` before
  applying the `ALTER TABLE` migration so repeated opens stay idempotent.
- `source_repository` is provenance metadata only. Branch-protection
  enforcement still depends on reviews, checks, signed commits, history
  shape, and admin policy; provenance does not grant merge or ref-operation
  bypasses.

Rollback/backfill:
- Before applying 0004 to a populated store, take a `VACUUM INTO` copy and
  keep it as the rollback target.
- Backfill the existing rows to the repository full name in the same migration
  transaction; the open helper may repeat the empty-string backfill safely and
  should be able to reopen the same database without changing already
  backfilled rows.
- The migration file carries timeout-guard metadata for the lock-sensitive
  `ALTER TABLE` so audit evidence can prove the shape change is not expected to
  wait indefinitely on traffic.
- If a rollback is needed after the field has been populated, restore the
  pre-migration database copy rather than deleting source provenance in place.
