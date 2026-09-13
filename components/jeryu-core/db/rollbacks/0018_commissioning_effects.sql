-- Retain complete commissioning operation and record history, including failures.
-- Keep incompatible writers stopped. An unfinished operation prevents ordinary
-- writes, schema migration and backfill until the qualified recovery procedure
-- completes; dropping a barrier or its records is never a rollback procedure.
-- Restore a verified complete pre-adoption package only without losing accepted
-- effects. After acceptance recover forward through the owning controller.
-- timeout-guard: lock_timeout = '5s'; statement_timeout = '60s'
SELECT '0018 rollback retains restoration barriers and requires compatible recovery' AS rollback_notice;
