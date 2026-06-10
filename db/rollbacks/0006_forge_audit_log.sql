-- Rollback for 0006_forge_audit_log: drop the audit table and its index.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
DROP INDEX IF EXISTS idx_forge_audit_log_subject_occurred_at;
DROP TABLE IF EXISTS forge_audit_log;
