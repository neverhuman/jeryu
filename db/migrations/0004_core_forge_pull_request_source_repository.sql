-- Add PR source provenance for fork/trust evaluation.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '5min'
ALTER TABLE pull_requests
ADD COLUMN source_repository TEXT NOT NULL DEFAULT '';

UPDATE pull_requests
SET source_repository = (
  SELECT r.owner || '/' || r.name
  FROM repositories r
  WHERE r.id = pull_requests.repo_id
)
WHERE source_repository = '';
