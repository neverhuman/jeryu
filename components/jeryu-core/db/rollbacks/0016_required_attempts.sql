-- Retain independent attempt, artifact, outbox and publisher history.
-- Keep incompatible writers stopped; never delete accepted authority receipts.
-- Restore a verified complete pre-adoption package only without losing accepted
-- effects. Otherwise recover forward through the owning controller.
-- timeout-guard: lock_timeout = '5s'; statement_timeout = '60s'
SELECT '0016 rollback retains required authority custody and needs compatible writers' AS rollback_notice;
