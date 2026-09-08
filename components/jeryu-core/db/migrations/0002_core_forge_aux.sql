-- Auxiliary durable rows for ForgeCore resources whose canonical tables in
-- 0001 intentionally keep the public shape compact.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
  login TEXT PRIMARY KEY CHECK (length(trim(login)) > 0),
  user_json TEXT NOT NULL CHECK (json_valid(user_json))
);

CREATE TABLE IF NOT EXISTS organizations (
  login TEXT PRIMARY KEY CHECK (length(trim(login)) > 0),
  organization_json TEXT NOT NULL CHECK (json_valid(organization_json))
);

CREATE TABLE IF NOT EXISTS teams (
  organization TEXT NOT NULL CHECK (length(trim(organization)) > 0),
  slug TEXT NOT NULL CHECK (length(trim(slug)) > 0),
  team_json TEXT NOT NULL CHECK (json_valid(team_json)),
  PRIMARY KEY (organization, slug),
  FOREIGN KEY (organization) REFERENCES organizations(login) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS labels (
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  label_json TEXT NOT NULL CHECK (json_valid(label_json)),
  PRIMARY KEY (repo_id, name)
);

CREATE TABLE IF NOT EXISTS issue_comments (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  issue_number INTEGER NOT NULL CHECK (issue_number > 0),
  comment_json TEXT NOT NULL CHECK (json_valid(comment_json)),
  FOREIGN KEY (repo_id, issue_number) REFERENCES issues(repo_id, number) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS review_comments (
  id TEXT PRIMARY KEY,
  review_id TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  pull_number INTEGER NOT NULL CHECK (pull_number > 0),
  comment_json TEXT NOT NULL CHECK (json_valid(comment_json)),
  FOREIGN KEY (repo_id, pull_number) REFERENCES pull_requests(repo_id, number) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS commit_statuses (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  sha TEXT NOT NULL CHECK (length(trim(sha)) > 0),
  status_json TEXT NOT NULL CHECK (json_valid(status_json))
);

CREATE TABLE IF NOT EXISTS codeowners (
  repo_id TEXT PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  contents TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS webhook_metadata (
  id TEXT PRIMARY KEY REFERENCES webhooks(id) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0)
);
