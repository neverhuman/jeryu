-- Retain the new column and every review event during forward repair.
-- An older writer discards dismissal targets during its full-state rewrite:
-- never run it against a populated migrated database.
-- Before activation, restore a verified consistent pre-0012 package only when
-- no accepted post-activation mutation would be lost. Afterwards recover forward.
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
SELECT '0012 rollback retains dismissal audit evidence; stop incompatible writers and recover forward' AS rollback_notice;
