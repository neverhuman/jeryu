-- Jeryu-web dev/e2e fixture store, migration 0001.
--
-- Until now the Playwright + component-test fixtures lived as hand-maintained
-- literals: `e2e/fixtures/mocks.ts` (page.route JSON handlers),
-- `e2e/fixtures/data/bootstrap.json`, and `src/test/mocks.ts`
-- (`makeBootstrapFixture`). Nothing enforced their relational shape, so a repo
-- summary could reference a pull request whose repo did not exist, a runner
-- could belong to no pool, and a permission string could be empty.
--
-- This migration is the durable, typed source of truth for that fixture data.
-- `db/seed/build-fixtures.mjs` applies it to an ephemeral SQLite database,
-- inserts the deterministic seed rows, and emits the derived JSON that the test
-- layer already consumes. Foreign keys and CHECK constraints are intentionally
-- explicit so migration analysis can prove the fixture boundary is a real
-- relational model and not an untyped JSON dump. This DB owns TEST fixtures
-- only; it is never wired into the SPA runtime bundle.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'

PRAGMA foreign_keys = ON;

-- --------------------------------------------------------------------------
-- Viewer + permissions (bootstrap.viewer, bootstrap.feature_flags)
-- --------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS viewers (
  id TEXT PRIMARY KEY,
  login TEXT NOT NULL UNIQUE CHECK (length(trim(login)) > 0),
  display_name TEXT,
  avatar_url TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS viewer_permissions (
  viewer_id TEXT NOT NULL REFERENCES viewers(id) ON DELETE CASCADE,
  permission TEXT NOT NULL CHECK (length(trim(permission)) > 0),
  PRIMARY KEY (viewer_id, permission)
);

-- Global bootstrap feature flags. Keyed by (viewer_id, flag) so a fixture can
-- model per-viewer flag overrides; the seed inserts the canonical global set.
CREATE TABLE IF NOT EXISTS feature_flags (
  viewer_id TEXT NOT NULL REFERENCES viewers(id) ON DELETE CASCADE,
  flag TEXT NOT NULL CHECK (length(trim(flag)) > 0),
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  PRIMARY KEY (viewer_id, flag)
);

-- --------------------------------------------------------------------------
-- Repositories -> pull_requests -> threads / checks
-- --------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS repositories (
  id TEXT PRIMARY KEY,
  host TEXT NOT NULL CHECK (length(trim(host)) > 0),
  owner TEXT NOT NULL CHECK (length(trim(owner)) > 0),
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  default_branch TEXT NOT NULL DEFAULT 'main' CHECK (length(trim(default_branch)) > 0),
  description TEXT,
  visibility TEXT NOT NULL DEFAULT 'private'
    CHECK (visibility IN ('public', 'internal', 'private')),
  family TEXT,
  repo_role TEXT CHECK (repo_role IS NULL OR repo_role IN ('public_portal', 'split_member')),
  health TEXT NOT NULL DEFAULT 'green'
    CHECK (health IN ('green', 'yellow', 'red')),
  topics_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(topics_json)),
  open_pull_requests INTEGER NOT NULL DEFAULT 0 CHECK (open_pull_requests >= 0),
  failing_checks INTEGER NOT NULL DEFAULT 0 CHECK (failing_checks >= 0),
  running_jobs INTEGER NOT NULL DEFAULT 0 CHECK (running_jobs >= 0),
  active_agents INTEGER NOT NULL DEFAULT 0 CHECK (active_agents >= 0),
  jankurai_score INTEGER CHECK (jankurai_score IS NULL OR (jankurai_score BETWEEN 0 AND 100)),
  jankurai_decision TEXT,
  jankurai_scored_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (host, owner, name)
);

-- Viewer's recent repositories (bootstrap.recent_repositories). Ordered join
-- that cascades from BOTH sides: dropping a viewer or a repository prunes the
-- membership row without orphaning it.
CREATE TABLE IF NOT EXISTS recent_repositories (
  viewer_id TEXT NOT NULL REFERENCES viewers(id) ON DELETE CASCADE,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  position INTEGER NOT NULL CHECK (position >= 0),
  PRIMARY KEY (viewer_id, repo_id),
  UNIQUE (viewer_id, position)
);

CREATE TABLE IF NOT EXISTS pull_requests (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  number INTEGER NOT NULL CHECK (number > 0),
  title TEXT NOT NULL CHECK (length(trim(title)) > 0),
  state TEXT NOT NULL CHECK (
    state IN ('draft', 'open', 'ready_for_review', 'approved', 'merged', 'closed')
  ),
  draft INTEGER NOT NULL DEFAULT 0 CHECK (draft IN (0, 1)),
  author TEXT NOT NULL CHECK (length(trim(author)) > 0),
  head_ref TEXT NOT NULL CHECK (length(trim(head_ref)) > 0),
  base_ref TEXT NOT NULL CHECK (length(trim(base_ref)) > 0),
  head_sha TEXT NOT NULL CHECK (length(trim(head_sha)) > 0),
  base_sha TEXT NOT NULL CHECK (length(trim(base_sha)) > 0),
  can_merge INTEGER NOT NULL DEFAULT 0 CHECK (can_merge IN (0, 1)),
  passport_status TEXT NOT NULL DEFAULT 'blocked'
    CHECK (passport_status IN ('pass', 'blocked')),
  passport_hash TEXT,
  required_approvals INTEGER NOT NULL DEFAULT 1 CHECK (required_approvals >= 0),
  approvals INTEGER NOT NULL DEFAULT 0 CHECK (approvals >= 0),
  changes_requested INTEGER NOT NULL DEFAULT 0 CHECK (changes_requested >= 0),
  unresolved_threads INTEGER NOT NULL DEFAULT 0 CHECK (unresolved_threads >= 0),
  labels_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(labels_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (repo_id, number),
  -- A merged PR must carry the passport hash the merge was gated on.
  CHECK (state <> 'merged' OR passport_hash IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS pull_request_threads (
  id TEXT PRIMARY KEY,
  pull_request_id TEXT NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
  author TEXT NOT NULL CHECK (length(trim(author)) > 0),
  path TEXT,
  line INTEGER CHECK (line IS NULL OR line > 0),
  body TEXT NOT NULL CHECK (length(trim(body)) > 0),
  resolved INTEGER NOT NULL DEFAULT 0 CHECK (resolved IN (0, 1)),
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pull_request_checks (
  id TEXT PRIMARY KEY,
  pull_request_id TEXT NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  status TEXT NOT NULL CHECK (status IN ('queued', 'in_progress', 'completed')),
  conclusion TEXT CHECK (
    conclusion IS NULL OR conclusion IN (
      'success', 'failure', 'neutral', 'cancelled', 'skipped', 'timed_out', 'action_required'
    )
  ),
  details_url TEXT,
  output_json TEXT CHECK (output_json IS NULL OR json_valid(output_json)),
  started_at TEXT NOT NULL,
  completed_at TEXT,
  UNIQUE (pull_request_id, name),
  -- A completed check must record a conclusion; an incomplete one must not.
  CHECK (
    (status = 'completed' AND conclusion IS NOT NULL)
    OR (status <> 'completed' AND conclusion IS NULL)
  )
);

-- --------------------------------------------------------------------------
-- Runner pools -> runners -> tasks (fleet bootstrap pool_activity)
-- --------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS runner_pools (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE CHECK (length(trim(name)) > 0),
  trust_tier TEXT NOT NULL DEFAULT 'trusted'
    CHECK (trust_tier IN ('trusted', 'partner', 'untrusted')),
  paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1)),
  configured_max_slots INTEGER NOT NULL DEFAULT 0 CHECK (configured_max_slots >= 0),
  tags_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(tags_json)),
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runners (
  id TEXT PRIMARY KEY,
  pool_id TEXT NOT NULL REFERENCES runner_pools(id) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  status TEXT NOT NULL DEFAULT 'idle'
    CHECK (status IN ('online', 'idle', 'busy', 'degraded', 'stuck', 'offline')),
  active_slots INTEGER NOT NULL DEFAULT 0 CHECK (active_slots >= 0),
  created_at TEXT NOT NULL,
  UNIQUE (pool_id, name)
);

CREATE TABLE IF NOT EXISTS runner_tasks (
  id TEXT PRIMARY KEY,
  runner_id TEXT NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
  -- Optional cross-reference to the repo the task builds. A repo can be dropped
  -- from the fixture without deleting fleet history, so this detaches (SET NULL)
  -- rather than cascading; it is nullable to make SET NULL well-defined.
  repo_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,
  kind TEXT NOT NULL CHECK (kind IN ('build', 'test', 'agent', 'mirror')),
  state TEXT NOT NULL DEFAULT 'queued'
    CHECK (state IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
  attempt INTEGER NOT NULL DEFAULT 1 CHECK (attempt >= 1),
  payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- --------------------------------------------------------------------------
-- Work items (the /work queue backing the viewer's actionable surface)
-- --------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS work_items (
  id TEXT PRIMARY KEY,
  viewer_id TEXT NOT NULL REFERENCES viewers(id) ON DELETE CASCADE,
  repo_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,
  kind TEXT NOT NULL CHECK (kind IN ('pull_request', 'issue', 'agent_run', 'check')),
  title TEXT NOT NULL CHECK (length(trim(title)) > 0),
  state TEXT NOT NULL DEFAULT 'todo'
    CHECK (state IN ('todo', 'in_progress', 'blocked', 'done')),
  priority TEXT NOT NULL DEFAULT 'medium'
    CHECK (priority IN ('low', 'medium', 'high', 'urgent')),
  url TEXT,
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- --------------------------------------------------------------------------
-- Read-path indexes (FK-covering indexes live in db/constraints/)
-- --------------------------------------------------------------------------

CREATE INDEX IF NOT EXISTS idx_repositories_family
  ON repositories(family);
CREATE INDEX IF NOT EXISTS idx_pull_requests_repo_state
  ON pull_requests(repo_id, state);
CREATE INDEX IF NOT EXISTS idx_work_items_viewer_state
  ON work_items(viewer_id, state, priority);
