# SQLite Constraints And Rollback Notes

Current persistence reconciles only explicitly owned State rows and columns
through connection-local staging tables. It updates changed rows in place,
inserts new keys, and deletes rows explicitly removed from State in child-first
order. Unknown columns, stable parent rowids and independently owned foreign-key
children survive ordinary saves. Constraints remain active; rejected writes
roll back the full transaction and the shared State. References below to older
full-table writers describe rollback hazards, not the maintained serializer.

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
- At most one explicit verdict per reviewer is effective at the current head.
  Comments preserve it; targeted dismissal semantics are specified under 0012.
  A later approval supersedes that reviewer's earlier changes
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

## 0012 Targeted review dismissal

The additive nullable `reviews.dismissed_review_id` stores the UUID of the
verdict removed by a new immutable `DISMISSED` event; its body stores the reason.
Existing rows remain unchanged with a NULL target. The application guards the
column addition with `PRAGMA table_info(reviews)` for repeated opens.

Core validates the same repository, PR, current head, owning actor and effective
explicit verdict while holding its state write guard. It rejects blank reasons,
generic target-less dismissal submissions and author self approvals. The shared
PR selector excludes inherited self approvals from required-count and CODEOWNERS
qualification. A targeted dismissal clears only its named current verdict and
never restores an older one. A historical target-less dismissal suppresses a
preceding approval but cannot erase a rejection; a later explicit decision can
establish a new verdict. Headless events remain unbound history.

No self-referential foreign key is added: the existing full-state rewrite
deletes and reinserts the history. Core enforces the target's identity and scope;
UUID parsing fails on corrupt stored values. Existing repository/PR foreign keys
remain. SQLite provides no row-level security here: the owning service and its
filesystem boundary remain responsible for tenant and actor access. A raw actor
login and an implicitly created profile are not authenticated account custody.

Before applying the migration, stop writers, hold the migration lock and retain
a verified consistent `VACUUM INTO` copy. The column addition needs the schema
write lock; use the recorded bounded lock and statement timeouts. There is no
data backfill. Do not start an older writer against the migrated database: its
full-state rewrite erases target bindings. The rollback notice retains all audit
rows; after accepted mutations, recover forward. Full state rollback is available
only when it would lose no accepted mutation.

## 0013 Repository creation journal

Core commits the immutable creation UUID, owner and complete original request
with repository metadata in the same SQLite transaction. Git materialization
and the durable completion write must both succeed before creation returns
success. A failed operation remains marked `materialized=false` and can be
retried using the same UUID and request. `repository_creations()` exposes these
records to the owning transport; it does not authenticate its caller.

The journal has no repository foreign key. Deletion retains its receipt so an
old request cannot recreate a removed identity or adopt a replacement at the
same name. Loading validates the JSON receipt and matching UUID. Every State
transaction includes the journal, including unrelated mutations. Existing rows have
no backfill: their creation identities cannot be inferred. Core completion does
not claim completion of the browser's README/family setup or Core/Work linking.

Before activation, stop writers, hold the migration lock and preserve a verified
consistent backup. The additive table needs a schema write lock; use the bounded
lock/statement budgets in its metadata. Do not start an older application against
the adopted store: it can leave creation receipts inconsistent with its writes.
Keep the table and recover forward after accepted mutations. Restore a verified
pre-migration package only when it loses no accepted mutation.

## 0013 Repository mutation restrictions

The original `0013_repository_mutation_blocks` migration is retained alongside
`0013_repository_creation`; startup applies both named additive migrations
idempotently. `repository_mutation_blocks.repo_id` references the immutable
repository UUID and its JSON payload records either read-only custody or an
operation awaiting reconciliation. No synthetic restriction is backfilled.

The typed maintenance API admits only nonempty reasons and evidence, or a
non-nil reconciliation operation UUID. A repeated identical block is idempotent;
ordinary APIs cannot clear it or replace it with another disposition. Reads
remain available. Every public writer reacquires authority and sorted UUID
guards and checks current identity and custody before touching State. Retained
creation retries and family completion use the same guards; an administrator
identity does not bypass them. Authentication and installed recovery authority
are separate boundaries, not granted by a reason or evidence string.

Before adoption, stop writers, retain a verified consistent backup and hold
the migration lock. Use the bounded lock and statement budgets in the migration
metadata. Preserve all restriction rows during rollback and keep incompatible
older writers stopped: they do not enforce these restrictions. After accepted
mutations or custody decisions, recover forward. A pre-adoption package may be
restored only when no accepted mutation or restriction is lost.

## 0014 Durable ref operations and committed-event outbox

`forge_ref_operations` binds a server-generated operation UUID to one immutable
repository UUID/idempotency key, complete serialized intent and canonical SHA256,
canonical qualification snapshot digest, marker identity and preparation audit.
The full snapshot preserves policy revision, policy, review/attempt UUIDs, actor
authorization and empty blockers plus any additional evidence. Digests are
recomputed on read; a supplied digest alone is never a qualification snapshot.
The private caller still must obtain and enforce the authoritative qualification;
storing a snapshot does not authenticate it.

Supported intents use explicit absent/exact preconditions and full nonzero
lowercase SHA1 object IDs. The current ref subset is ASCII `refs/heads/` and
`refs/tags/`, excluding Git revision syntax, invalid components and the reserved
`refs/jeryu/operations/<UUID>` marker namespace. Each ref occurs once. The future
Git backend must also enforce that marker namespace against all other writers.

The only ordinary transition is prepared to committed, aborted_not_applied or
reconciliation_required. A correct recorded marker plus every exact result ref
classifies committed; exact predecessors without a marker or other application
evidence classify not-applied. Missing, inconsistent or unavailable observations
require reconciliation. All terminal observations are immutable. An identical
retry returns its original receipt; any different outcome conflicts. A partial
unique index blocks another prepared operation while a repository's prepared or
reconciliation-required row remains unresolved. No ordinary method clears it.

Private outcome persistence uses one transaction for the optional proposed State
closure, operation outcome, immutable audit and committed-event outbox. The State
closure runs only for a new committed outcome. Shared State is replaced only
after commit, so faults leave both memory and SQLite at the predecessor. An
outbox event has one stable UUID per committed operation and its immutable full
payload. Pending delivery survives restart and State saves. Acknowledgement is
private, requires that committed operation and binds an immutable delivery-receipt
digest; duplicate acknowledgement returns the original record. No event or
acknowledgement is manufactured for a prepared, aborted or quarantined operation.

The operation and audit have no catalog FK. Outbox references the operation with
ON DELETE RESTRICT and no repository FK. Ordinary State reconciliation never
owns these rows; intentional repository deletion preserves their UUID-based
historical custody. These private primitives do not yet fence ordinary writers,
dispatch Git, close production PRs, authenticate an actor, emit required checks,
deliver effects or replace the existing merge routes. The next execute_merge and
receive-pack integration must hold the coordinator continuously across current
qualification, prepared intent, Git CAS/marker transaction and durable outcome.

Migration/backfill/recovery: the forward migration creates absent tables,
indexes and guards idempotently; it invents no historical operation or event.
Before installed use, stop incompatible writers and retain a consistent complete
Git/database/artifact/configuration recovery package and its tested restoration.
Do not drop a live journal, lose accepted effects or run an incompatible older
binary to undo migration. Preserve accepted Git advances and recover forward.
Resolving reconciliation_required requires a separately designed and reviewed
authority path; this migration supplies none. Source transaction/regression tests
are distinct from installed migration and crash-recovery qualification.

## Authenticated review challenges and events (0015)

Challenges and events are independently persisted. A challenge UUID retains its
full canonical snapshot, digest, nonce and expiry. The snapshot binds guarded
catalog repository UUIDs and PR identity to direct source/base refs, full SHA1
commits and trees, actual physical repository and Git executable identities,
current policy revision/content, account/credential/epoch and grant records,
and the exact advisory and bound evidence visible at creation. No caller JSON
supplies a trusted observation. Review mutation requires a Core-minted actor
whose actual credential still exists and whose account, epoch and permissions
remain eligible under the same authority/repository guards.

An accepted event has one UUID (also its compatibility review ID), a monotonic
sequence per repository/PR UUID, one consumed challenge and immutable content.
Nonce consumption, independent event, compatibility review/inline comments and
audit commit together with the proposed State. Failed persistence publishes no
State change; identical accepted retry returns its original event, while changed
input conflicts. Caller expected_head_sha participates in the request digest and
must equal the immutable challenge head under guards. Expired pending challenges
cannot create a new event; expiration never rewrites an accepted event.

No catalog FK can erase challenge, event or outcome audit history. The tables
are outside State-save ownership; the loaded event cache is read-only outside
the composed event transaction. Append-only triggers prevent event replacement
and challenge rebinding/deletion. Review comments never change a verdict. Only
a fresh authenticated decision supersedes it; a targeted dismissal may clear
the authenticated actor's own current verdict and never recovers an older one.
Authorization or policy changes can remove positive qualification without
erasing an existing rejection or any historical event.

Migration is additive and idempotent with no invented historical bindings. Every
legacy review (even one with a full head) and every legacy status/check row stays
advisory. The three old split merge methods refuse before any dispatch or State
mutation. A successful source review is not merge qualification: authoritative
check attempts, continuous Git mutation exclusion, the Core-owned executor and
installed recovery qualification remain subsequent work. Retain a consistent
full-state backup and restoration receipt before installation; do not run an
incompatible older writer or discard accepted history to undo migration.

## Required attempt reservations and received evidence (0016)

`forge_required_attempts` owns server UUIDs and positive monotonically increasing
ordinals per repository UUID, exact commit and context. An idempotency key is
unique within its repository; all immutable reservation bindings and fixed
expiry must match on replay. Bindings retain exact commit/tree, opaque actor
principal/credential/epoch, publisher/runtime, enrollment, evidence contract and
authority origin. No caller-supplied historical status receives these bindings.

The newest reservation governs immediately. Pending, failed, cancelled and
expired reservations cannot fall back to an earlier success. Completion is
accepted only from reservation time up to, but excluding, fixed expiry. An
identical already-terminal retry returns its original receipt, even later;
different input conflicts. The future caller must independently revalidate its
current enrolled publisher, credentials, custody and origin before these private
persistence methods are invoked. The store does not authenticate request DTOs.

`forge_required_attempt_artifacts` retains actual receiving bytes and Core-made
SHA-256/size/name identities; payload paths and caller hashes are not receiving
evidence. The current bounded envelope admits 1–64 uniquely named nonempty
artifacts totaling at most 16 MiB. Large build artifacts need separately qualified
external custody and a bound receipt within that envelope; this limit is not a
claim of large-artifact transport support. Terminal result, artifact bytes, audit
and `forge_required_attempt_outbox` commit atomically. Reads obtain one SQLite
snapshot, including newest selection, and verify artifact bytes/outbox against
the immutable result.

All three tables are outside State-save ownership and carry no catalog FK. They
survive unrelated full-state saves, reopen and catalog deletion. The additive
migration invents no attempts, grants or publisher enrollments. Before installed
use, retain a consistent full-state recovery package and verified restoration,
stop incompatible writers, and recover forward after accepted effects. The
private hooks and source tests do not qualify publisher enrollment, authenticated
routes, commissioning origin, merge execution or installed behavior.

## Required publisher enrollment history (0017)

`forge_required_publishers` owns immutable publisher UUID/revision, enrollment
hash and installation operation identity. The first record requires expected
absence and revision one; every successor requires the exact preceding hash and
next revision. Only the latest revision may authorize an attempt. An operation
retry retains the original identity/content; conflicting reuse refuses.

Revocation targets the exact current enrollment and appends audit in the same
transaction as its durable record update. Identical retries retain the original
revocation; changed requests refuse. Dedicated history survives ordinary State
saves and runtime restart, with no repository/account cascade deletion. Reading
an enrollment validates indexed identities, canonical content digest and bounds
under one SQLite read snapshot.

The private installer does not authenticate request data. Its future caller is
the root-owned commissioning verifier under the authority guard, after detached
independent acceptance and current actor/key/credential checks. The attached
Core authority service starts absent on restart. Public reserve/complete/snapshot
methods revalidate opaque credentials, exact source scope and enrolled bindings;
no transport may install arbitrary publisher JSON. Test-only gate implementations
are not production trust or enrollment evidence.

Migration is additive, idempotent and has no historical backfill. Before installed
migration retain a complete consistent backup with a verified restore rehearsal,
keep incompatible writers stopped and preserve all enrollment/audit history.
After accepted effects, recover forward instead of removing rows or restoring an
older writer. Eleven new owning fixtures are prepared but unexecuted; actual
signing, installed custody and receiving transport qualification remain open.

## Commissioning restoration journal and admission barrier (0018)

`forge_commissioning_operations` retains exact contract, repository and backing
pair identities with one active operation per actual storage runtime. Independent
`forge_commissioning_records` preserve the complete immutable revision/hash chain,
step admissions, received outcome bytes and final closure. SQL constraints refuse
record replacement/deletion, scope rewriting and closure without a terminal
record. Readback reconstructs the entire chain, including purportedly closed
operations; a damaged header, orphan or partial schema cannot grant admission.

Reservation drains ordinary guarded effects before recording the barrier. The
barrier survives completed steps and restart; State persistence, ordinary Git
callbacks, audit writes, migrations and backfills remain blocked until a verified
complete operation closes atomically. Identical retries return their original
recorded responses only after validating the full current chain. A lost terminal
write rolls back the terminal record and barrier transition together.

The ordered target plan, exact revision/hash, fixed window and current operator
scope govern each new step. Received late/failed/unknown outcomes and separately
authenticated recovery-recorder evidence remain durable without granting further
effects or closure. Strict duplicate-free, bounded canonical JSON is a content
contract; it supplies no signature, enrollment or installation authority.

Ordinary barrier inspection uses an actual custody-checked read-only SQLite
connection so an unchanged-visibility no-op does not require database writes.
Active barriers still refuse that no-op. Installation-only authority attachment
and authenticated recovery readback can resume without issuing new credentials
or allowing ordinary writes. Production signing, recovery dispatch and full
installed campaigns remain unqualified; test fixtures cannot grant that trust.

Migration is additive and has no historical backfill. Preserve and restore-test
a complete consistent state package before adoption, stop incompatible writers,
retain all recovery records and recover forward after accepted effects. Dropping
these tables, forging closure or rolling accepted Git backward is prohibited.
