-- Rollback for 0005_repository_family: drop the grouping column.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
ALTER TABLE repositories DROP COLUMN family;
