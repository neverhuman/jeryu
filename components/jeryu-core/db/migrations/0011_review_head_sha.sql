-- Additive exact-head binding for pull-request review audit rows.
-- Historical rows intentionally remain NULL and therefore stale.
ALTER TABLE reviews ADD COLUMN head_sha TEXT;
