PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS work_items (
    number INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    repo_id TEXT,
    repo_host TEXT,
    repo_owner TEXT,
    repo_name TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    body TEXT,
    status TEXT NOT NULL CHECK (status IN ('backlog', 'ready', 'in_progress', 'blocked', 'in_review', 'done', 'canceled')),
    kind TEXT NOT NULL CHECK (kind IN ('task', 'bug', 'chore', 'docs', 'ci')),
    priority TEXT NOT NULL CHECK (priority IN ('p0', 'p1', 'p2', 'p3', 'p4')),
    labels_json TEXT NOT NULL,
    assignees_json TEXT NOT NULL,
    issue_owner TEXT,
    issue_repo TEXT,
    issue_number INTEGER CHECK (issue_number IS NULL OR issue_number > 0),
    issue_url TEXT,
    pull_requests_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS work_items_issue_unique
ON work_items(issue_owner, issue_repo, issue_number)
WHERE issue_owner IS NOT NULL AND issue_repo IS NOT NULL AND issue_number IS NOT NULL;

CREATE INDEX IF NOT EXISTS work_items_repo_idx ON work_items(repo_id);
CREATE INDEX IF NOT EXISTS work_items_status_idx ON work_items(status);

CREATE TABLE IF NOT EXISTS work_comments (
    id TEXT PRIMARY KEY,
    work_number INTEGER NOT NULL REFERENCES work_items(number) ON DELETE CASCADE,
    author_json TEXT NOT NULL,
    body TEXT NOT NULL CHECK (length(trim(body)) > 0),
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS work_comments_work_idx ON work_comments(work_number, created_at);
