-- Retain creation receipts, including deleted repository UUIDs. Dropping this
-- table or running an older writer loses retry and deletion identity bindings.
-- Before activation, a verified consistent pre-0013 package may be restored
-- only if no accepted subsequent mutation is lost. Otherwise recover forward.
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
SELECT '0013 rollback retains creation identity; stop incompatible writers and recover forward' AS rollback_notice;
