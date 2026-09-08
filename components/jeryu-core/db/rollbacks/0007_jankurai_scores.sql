-- Rollback for 0007_jankurai_scores: drop the audit-score store.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
DROP INDEX IF EXISTS idx_jankurai_scores_repo_branch;
DROP TABLE IF EXISTS jankurai_scores;
