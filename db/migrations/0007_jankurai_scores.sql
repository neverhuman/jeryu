-- Durable jankurai audit scores per repository branch/commit.
--
-- Until now scores were ephemeral CI artifacts (.jankurai/repo-score.json);
-- this table is the store behind POST /api/v1/repos/:id/jankurai-scores so
-- the repos list can badge every repository's default branch. `score` is
-- nullable: decision='tool-failed' records an audit the tool could not score
-- (e.g. non-Rust repos) without inventing a number.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
CREATE TABLE IF NOT EXISTS jankurai_scores (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  branch TEXT NOT NULL CHECK (length(trim(branch)) > 0),
  commit_sha TEXT NOT NULL CHECK (length(trim(commit_sha)) > 0),
  score INTEGER CHECK (score IS NULL OR (score BETWEEN 0 AND 100)),
  hard_findings INTEGER NOT NULL DEFAULT 0 CHECK (hard_findings >= 0),
  decision TEXT NOT NULL CHECK (length(trim(decision)) > 0),
  caps_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(caps_json)),
  report_json TEXT CHECK (report_json IS NULL OR json_valid(report_json)),
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jankurai_scores_repo_branch
  ON jankurai_scores(repo_id, branch, created_at);
