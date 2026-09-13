-- Keep immutable operation bindings, outcomes, audit and pending effects.
-- Do not reverse an accepted Git advance to conceal a metadata failure.
-- Keep incompatible writers stopped. Restore a verified pre-adoption complete
-- package only if it loses no accepted Git, database, artifact or custody state;
-- otherwise recover forward under the separately reviewed recovery authority.
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
SELECT '0014 rollback retains operation and delivery custody; preserve accepted Git and recover forward' AS rollback_notice;
