-- Jeryu durable forge truth, migration 0001.
-- Foreign key and CHECK constraints are intentionally explicit so migration
-- analysis can prove the DB boundary is more than an untyped JSON dump.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS repositories (
  id TEXT PRIMARY KEY,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  full_name TEXT NOT NULL UNIQUE,
  private INTEGER NOT NULL CHECK (private IN (0, 1)),
  description TEXT,
  default_branch TEXT NOT NULL CHECK (length(default_branch) > 0),
  archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
  disabled INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (owner, name)
);

CREATE TABLE IF NOT EXISTS issues (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  number INTEGER NOT NULL CHECK (number > 0),
  title TEXT NOT NULL CHECK (length(trim(title)) > 0),
  body TEXT,
  state TEXT NOT NULL CHECK (state IN ('open', 'closed')),
  author TEXT NOT NULL CHECK (length(trim(author)) > 0),
  labels_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(labels_json)),
  assignees_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(assignees_json)),
  milestone TEXT,
  comments INTEGER NOT NULL DEFAULT 0 CHECK (comments >= 0),
  pull_request_json TEXT CHECK (pull_request_json IS NULL OR json_valid(pull_request_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  closed_at TEXT,
  UNIQUE (repo_id, number)
);

CREATE TABLE IF NOT EXISTS pull_requests (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  number INTEGER NOT NULL CHECK (number > 0),
  issue_number INTEGER NOT NULL CHECK (issue_number > 0),
  title TEXT NOT NULL CHECK (length(trim(title)) > 0),
  body TEXT,
  state TEXT NOT NULL CHECK (
    state IN (
      'draft',
      'open',
      'ready_for_review',
      'blocked_by_policy',
      'blocked_by_checks',
      'approved',
      'queued',
      'speculative_merge_testing',
      'mergeable',
      'merged',
      'closed'
    )
  ),
  draft INTEGER NOT NULL CHECK (draft IN (0, 1)),
  author TEXT NOT NULL CHECK (length(trim(author)) > 0),
  head_json TEXT NOT NULL CHECK (json_valid(head_json)),
  base_json TEXT NOT NULL CHECK (json_valid(base_json)),
  mergeable INTEGER NOT NULL CHECK (mergeable IN (0, 1)),
  mergeable_state TEXT NOT NULL,
  merged INTEGER NOT NULL CHECK (merged IN (0, 1)),
  merged_at TEXT,
  merge_commit_sha TEXT,
  commits_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(commits_json)),
  changed_files_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(changed_files_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (repo_id, number),
  FOREIGN KEY (repo_id, issue_number) REFERENCES issues(repo_id, number)
);

CREATE TABLE IF NOT EXISTS reviews (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  pull_number INTEGER NOT NULL CHECK (pull_number > 0),
  author TEXT NOT NULL CHECK (length(trim(author)) > 0),
  state TEXT NOT NULL CHECK (state IN ('APPROVED', 'CHANGES_REQUESTED', 'COMMENTED', 'DISMISSED')),
  body TEXT,
  submitted_at TEXT NOT NULL,
  FOREIGN KEY (repo_id, pull_number) REFERENCES pull_requests(repo_id, number)
);

CREATE TABLE IF NOT EXISTS check_runs (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  head_sha TEXT NOT NULL CHECK (length(trim(head_sha)) > 0),
  status TEXT NOT NULL CHECK (status IN ('queued', 'in_progress', 'completed')),
  conclusion TEXT CHECK (
    conclusion IS NULL OR conclusion IN (
      'action_required',
      'cancelled',
      'failure',
      'neutral',
      'success',
      'skipped',
      'stale',
      'timed_out'
    )
  ),
  details_url TEXT,
  output_json TEXT CHECK (output_json IS NULL OR json_valid(output_json)),
  started_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE TABLE IF NOT EXISTS branch_protection_rules (
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  branch TEXT NOT NULL CHECK (length(trim(branch)) > 0),
  rule_json TEXT NOT NULL CHECK (json_valid(rule_json)),
  PRIMARY KEY (repo_id, branch)
);

CREATE TABLE IF NOT EXISTS webhooks (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  config_json TEXT NOT NULL CHECK (json_valid(config_json)),
  events_json TEXT NOT NULL CHECK (json_valid(events_json)),
  active INTEGER NOT NULL CHECK (active IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
  id TEXT PRIMARY KEY,
  hook_id TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  event TEXT NOT NULL CHECK (length(trim(event)) > 0),
  target_url TEXT NOT NULL CHECK (length(trim(target_url)) > 0),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  signature_256 TEXT,
  delivered INTEGER NOT NULL DEFAULT 0 CHECK (delivered IN (0, 1)),
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS repo_counters (
  repo_id TEXT PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  issue_next INTEGER NOT NULL DEFAULT 1 CHECK (issue_next > 0),
  pull_next INTEGER NOT NULL DEFAULT 1 CHECK (pull_next > 0)
);

