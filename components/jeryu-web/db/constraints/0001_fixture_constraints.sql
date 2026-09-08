-- Constraint companion for migration 0001_fixture_model.
--
-- The relational invariants themselves (CHECK, FOREIGN KEY ... ON DELETE
-- CASCADE / SET NULL, json_valid) are declared inline in
-- db/migrations/0001_fixture_model.sql because SQLite only accepts table
-- constraints at CREATE TABLE time. This file is the constraint CATALOG and
-- the enforcement companion the seed applies AFTER the migration:
--
--   1. It re-asserts PRAGMA foreign_keys = ON so cascade/detach semantics are
--      actually enforced for the connection that seeds and reads back rows.
--   2. It creates the covering indexes SQLite does NOT create automatically for
--      foreign-key child columns. Without them every ON DELETE CASCADE / SET
--      NULL does a full table scan of the child, and the fixture read paths
--      (repo -> pulls, pull -> threads/checks, pool -> runners -> tasks) scan
--      too. These indexes make both cheap and are the mechanical proof that
--      each FK is real.
--   3. The header of each section restates the invariant the index backs; the
--      prose rationale, rollback, backfill, and lock-safety notes live in
--      db/constraints.md.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'

PRAGMA foreign_keys = ON;

-- viewer_permissions.viewer_id -> viewers(id) ON DELETE CASCADE
--   (covered by the PRIMARY KEY (viewer_id, permission) leftmost column)
-- feature_flags.viewer_id -> viewers(id) ON DELETE CASCADE
--   (covered by the PRIMARY KEY (viewer_id, flag) leftmost column)

-- recent_repositories: cascades from both viewers and repositories.
CREATE INDEX IF NOT EXISTS idx_recent_repositories_repo
  ON recent_repositories(repo_id);

-- pull_requests.repo_id -> repositories(id) ON DELETE CASCADE
CREATE INDEX IF NOT EXISTS idx_pull_requests_repo
  ON pull_requests(repo_id);

-- pull_request_threads.pull_request_id -> pull_requests(id) ON DELETE CASCADE
CREATE INDEX IF NOT EXISTS idx_pull_request_threads_pull
  ON pull_request_threads(pull_request_id);

-- pull_request_checks.pull_request_id -> pull_requests(id) ON DELETE CASCADE
--   (covered by UNIQUE (pull_request_id, name) leftmost column; add an explicit
--    index too so the intent is legible in the catalog)
CREATE INDEX IF NOT EXISTS idx_pull_request_checks_pull
  ON pull_request_checks(pull_request_id);

-- runners.pool_id -> runner_pools(id) ON DELETE CASCADE
CREATE INDEX IF NOT EXISTS idx_runners_pool
  ON runners(pool_id);

-- runner_tasks.runner_id -> runners(id) ON DELETE CASCADE
CREATE INDEX IF NOT EXISTS idx_runner_tasks_runner
  ON runner_tasks(runner_id);

-- runner_tasks.repo_id -> repositories(id) ON DELETE SET NULL
CREATE INDEX IF NOT EXISTS idx_runner_tasks_repo
  ON runner_tasks(repo_id);

-- work_items.viewer_id -> viewers(id) ON DELETE CASCADE
--   (covered by idx_work_items_viewer_state leftmost column in the migration)
-- work_items.repo_id -> repositories(id) ON DELETE SET NULL
CREATE INDEX IF NOT EXISTS idx_work_items_repo
  ON work_items(repo_id);
