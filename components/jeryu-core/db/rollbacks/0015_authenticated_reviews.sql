-- Preserve immutable review events, consumed challenges, audit and compatibility
-- projections. Keep older writers stopped; they cannot enforce bound authority.
-- Restore a verified complete pre-adoption package only when it loses no accepted
-- effects. Otherwise recover forward without inventing authentication history.
-- timeout-guard: lock_timeout = '5s'; statement_timeout = '60s'
SELECT '0015 rollback retains authenticated review custody and requires compatible writers' AS rollback_notice;
