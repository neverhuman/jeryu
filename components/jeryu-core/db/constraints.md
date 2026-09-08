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
- Backfill the existing rows to the repository full name in the same
  transaction; the open helper may repeat the empty-string backfill safely and
  should be able to reopen the same database without changing already
  backfilled rows.
- The migration file carries timeout-guard metadata for the lock-sensitive
  `ALTER TABLE` so audit evidence can prove the shape change is not expected to
  wait indefinitely on traffic.
- If a rollback is needed after the field has been populated, restore the
  pre-migration database copy rather than deleting source provenance in place.

## 0008 Public portal auth and repo grants

The eighth migration adds durable web account credentials, hashed sessions,
hashed personal access tokens, and per-repository grants.

- `user_accounts.login` references the profile `users.login` row and stores
  Argon2id PHC password hashes only.
- `user_accounts.must_change_password` marks bootstrap and admin-reset
  credentials as temporary until the user changes the password through the
  typed forge API.
- `web_sessions.token_hash` and `personal_access_tokens.token_hash` are
  SHA-256 hashes of high-entropy bearer values; plaintext tokens are never
  stored.
- `web_sessions.csrf_token` is a per-session random value required by the HTTP
  edge for unsafe cookie-authenticated requests; legacy rows from an older 0008
  shape receive an empty value and cannot pass CSRF validation for unsafe
  requests.
- `repo_access_grants` keys access by `(login, repo_id)` and cascades with both
  the account and repository.
- Grant values are constrained to `read`, `write`, or `admin`; global
  administrator users are represented by `user_accounts.role = 'admin'`.
- The full-state rewrite threads every new table through `State`, `load_state`,
  `persist_state`, and `delete_all` so account state survives unrelated forge
  mutations.

Rollback/backfill:
- Before applying 0008 to a populated store, take a `VACUUM INTO` copy and keep
  it as the rollback target.
- Reopening an existing 0008 store adds `must_change_password` and `csrf_token`
  with safe defaults when those columns are absent. No credential material is
  generated for existing profile-only users.
- Admin password reset revokes that user's sessions and personal access tokens
  in the typed forge state before persistence.
- Rollback drops the additive auth/grant tables only for pre-production use; in
  a populated store, restore the pre-migration database copy instead of
  deleting account rows in place.

## 0009 Repository transfer journals and aliases

The ninth migration adds durable two-phase repository-transfer journals and
read-only old-slug aliases. The application prepares a journal before moving
storage, then records exactly one terminal `committed` or `failed` result.

Constraint policy:
- `repository_transfer_journal.transaction_id` is the primary key and each
  `idempotency_key` is unique. Both identify one immutable transfer attempt.
- Every journal references `repositories.id` with `ON DELETE CASCADE`.
  `status` is closed to `prepared`, `committed`, or `failed`; `receipt_json`,
  when present, must be valid JSON.
- `repository_aliases` is keyed by the old `(owner, name)` slug, references
  both the immutable repository UUID and its transfer transaction, and rejects
  duplicate repository/old-slug triples.
- Preparation rejects a destination that collides with either a canonical
  repository slug or an existing alias. Commit rechecks the destination inside
  the same locked state transition before re-keying any repository-owned row.
- `repository_transfer_journal` and `repository_aliases` are threaded through
  `State`, `load_state`, `persist_state`, and `delete_all`; unrelated full-state
  rewrites must preserve both tables.
- A failed journal is terminal. Replaying the exact failure reason returns the
  original record unchanged; a different reason is a conflict and cannot
  replace the original completion timestamp or cause.

Rollback/backfill:
- The migration is additive and requires no backfill. Both tables start empty
  and are populated only by explicit transfer operations.
- Before applying 0009 to a populated store, take a `VACUUM INTO` copy while
  holding the application migration lock and retain it as the restoration
  target.
- The staged rollback is non-destructive: disable new transfer operations,
  retain both recovery tables, and roll forward after repair. If schema removal
  is unavoidable, restore the pre-migration copy instead of dropping live
  journals or aliases.

## 0010 Account lifecycle, invitations, and owner bootstrap

The tenth migration extends durable accounts with canonical identity and
credential epochs, and adds hash-only invitation, activation-challenge, and
permanent first-owner bootstrap state.

Constraint policy:
- Existing account logins are preflighted before any shape change. ASCII
  case-fold collisions and every non-canonical login fail the open; the
  migration never silently renames identities or rewrites foreign keys.
- `user_accounts.display_name` backfills from the canonical login.
  `user_accounts.status` is closed to `pending_activation`, `pending_mfa`,
  `active`, `disabled`, or `locked`, and existing accounts backfill to
  `active` for compatibility. `auth_epoch` is nonnegative and starts at zero.
- Sessions and personal access tokens persist the account epoch at issuance.
  Authentication requires both an active account and an exact epoch match.
- Invitation activation secrets and activation challenges are random 256-bit
  values stored only as 64-character SHA-256 hashes. Administrative listing
  models omit both hashes and plaintext values.
- Invitation expiry is at most 24 hours. A partial unique index permits only
  one unconsumed, unrevoked reservation per canonical login; the typed create
  path revokes expired reservations in the same state transaction before
  inserting a successor.
- Activation attempts are bounded, challenges are short-lived and single-use,
  and completion marks both the invitation and challenge consumed in the same
  state transaction that creates the `pending_mfa` account.
- `owner_bootstrap_state` has exactly one singleton row. Completion of the
  first bootstrap owner invitation changes it permanently to consumed; the
  typed API refuses later owner bootstrap invitations even if the first owner
  is disabled or removed from runtime access.
- `account_invitations`, `account_activation_challenges`, and
  `owner_bootstrap_state` are threaded through `State`, `load_state`,
  `persist_state`, and `delete_all`; unrelated full-state rewrites preserve
  them.

Rollback/backfill:
- Before applying 0010 to a populated store, hold the application migration
  lock and create a verified `VACUUM INTO` copy. Record the existing-account
  count, the canonical-login preflight result, and the copy hash in the
  migration receipt.
- The only data backfill sets existing display names to their login and applies
  the safe `active`/epoch-zero defaults to existing accounts and credentials.
  No invitation, activation secret, challenge, bootstrap credential, session,
  or PAT is generated by migration.
- If canonical-login preflight fails, leave the database unopened and resolve
  the collision through a separately reviewed identity-disposition procedure;
  do not edit login rows ad hoc during startup.
- Rollback is restore-only for a populated store. Stop writers, restore the
  verified pre-0010 copy, and verify its refs and metadata before restarting;
  do not drop invitation or bootstrap tables in place because that could
  re-enable consumed bootstrap authority or lose revocation evidence.

## 0011 Review exact-head binding

The eleventh migration adds nullable `reviews.head_sha` so review audit history
is distinct from current-head merge authority.

Constraint policy:
- Existing rows remain `NULL`; they are retained as audit history but are stale
  for every current pull-request head.
- Every newly created review captures the pull request's exact head while Core
  holds the state write lock. HTTP callers also supply that head as an
  optimistic-concurrency guard, and a moved head rejects the review.
- At most one review per reviewer is effective: the latest non-dismissed row at
  the current head. A later approval supersedes that reviewer's earlier changes
  request at the same head; another reviewer's current changes request remains a
  merge blocker.
- Head movement invalidates approvals and changes requests without deleting or
  rewriting any review row.

Rollback/backfill:
- Before applying 0011 to a populated store, hold the application migration lock
  and retain a verified `VACUUM INTO` copy.
- There is deliberately no backfill. Inferring historical review heads would
  turn unauditable guesses into merge authority.
- The additive column remains during an application rollback. If schema removal
  is unavoidable, restore the verified pre-0011 copy rather than rebuilding the
  live reviews table in place.
