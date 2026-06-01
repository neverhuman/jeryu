-- Persisted repository README markdown for the local publish flow.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS repository_readmes (
  repo_id TEXT PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  contents TEXT NOT NULL
);
