-- Retain all recorded restrictions. Older writers do not enforce them and
-- must remain stopped. Do not drop this table to restore mutation access.
-- Restore a verified pre-adoption package only if it loses no accepted writes
-- or custody decisions; otherwise recover forward under the existing blocks.
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
SELECT '0013 rollback retains mutation custody; stop incompatible writers and recover forward' AS rollback_notice;
