-- Targeted review dismissals are additive, immutable audit events.
-- Historical target-less dismissals remain NULL; no target is inferred.
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
ALTER TABLE reviews ADD COLUMN dismissed_review_id TEXT;
