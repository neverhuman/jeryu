-- Persisted repository family for UI grouping (e.g. jmcp-split, veox-split).
--
-- NULL means the repository is a standalone project. The seed backfill in
-- apply_migration_0005 derives families from import provenance (a `*-split`
-- parent directory in the "imported from <path>" description) plus the known
-- name-prefix groups; thereafter family is plain data edited via the API.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
ALTER TABLE repositories
ADD COLUMN family TEXT CHECK (family IS NULL OR length(trim(family)) > 0);
