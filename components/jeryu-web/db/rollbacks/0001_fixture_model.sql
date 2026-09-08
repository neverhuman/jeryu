-- Rollback for 0001_fixture_model: drop the dev/e2e fixture store.
--
-- Safe because this store holds only synthetic, regenerable fixture data:
-- `db/seed/build-fixtures.mjs` rebuilds the whole schema and reseeds it from
-- source on demand, so there is no durable truth to preserve. Tables are
-- dropped child-first so the ON DELETE CASCADE foreign keys never dangle
-- mid-teardown even when PRAGMA foreign_keys is ON.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'

DROP INDEX IF EXISTS idx_work_items_viewer_state;
DROP INDEX IF EXISTS idx_pull_requests_repo_state;
DROP INDEX IF EXISTS idx_repositories_family;

DROP TABLE IF EXISTS work_items;
DROP TABLE IF EXISTS runner_tasks;
DROP TABLE IF EXISTS runners;
DROP TABLE IF EXISTS runner_pools;
DROP TABLE IF EXISTS pull_request_checks;
DROP TABLE IF EXISTS pull_request_threads;
DROP TABLE IF EXISTS pull_requests;
DROP TABLE IF EXISTS recent_repositories;
DROP TABLE IF EXISTS repositories;
DROP TABLE IF EXISTS feature_flags;
DROP TABLE IF EXISTS viewer_permissions;
DROP TABLE IF EXISTS viewers;
