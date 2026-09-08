-- Durable public-portal authentication and per-repository access grants.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS user_accounts (
  login TEXT PRIMARY KEY REFERENCES users(login) ON DELETE CASCADE,
  password_hash TEXT NOT NULL CHECK (password_hash LIKE '$argon2id$%'),
  role TEXT NOT NULL CHECK (role IN ('admin', 'user')),
  must_change_password INTEGER NOT NULL DEFAULT 0 CHECK (must_change_password IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_sessions (
  id TEXT PRIMARY KEY,
  login TEXT NOT NULL REFERENCES user_accounts(login) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE CHECK (length(token_hash) = 64),
  csrf_token TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_web_sessions_login
ON web_sessions(login);

CREATE TABLE IF NOT EXISTS personal_access_tokens (
  id TEXT PRIMARY KEY,
  login TEXT NOT NULL REFERENCES user_accounts(login) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  token_hash TEXT NOT NULL UNIQUE CHECK (length(token_hash) = 64),
  created_at TEXT NOT NULL,
  expires_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_personal_access_tokens_login
ON personal_access_tokens(login);

CREATE TABLE IF NOT EXISTS repo_access_grants (
  login TEXT NOT NULL REFERENCES user_accounts(login) ON DELETE CASCADE,
  repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  access TEXT NOT NULL CHECK (access IN ('read', 'write', 'admin')),
  granted_by TEXT NOT NULL CHECK (length(trim(granted_by)) > 0),
  granted_at TEXT NOT NULL,
  PRIMARY KEY (login, repo_id)
);

CREATE INDEX IF NOT EXISTS idx_repo_access_grants_repo
ON repo_access_grants(repo_id);
