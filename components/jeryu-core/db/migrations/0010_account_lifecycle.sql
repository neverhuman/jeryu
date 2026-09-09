-- Durable invitation, activation, and one-time owner-bootstrap state.
--
-- Existing-table columns are added by the guarded Rust migration because
-- SQLite has no portable `ADD COLUMN IF NOT EXISTS` form.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS account_invitations (
  id TEXT PRIMARY KEY,
  canonical_login TEXT NOT NULL,
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
  activation_secret_hash TEXT NOT NULL UNIQUE CHECK (length(activation_secret_hash) = 64),
  issuer_principal TEXT NOT NULL CHECK (length(trim(issuer_principal)) > 0),
  intended_role TEXT NOT NULL CHECK (intended_role IN ('admin', 'user')),
  intended_teams_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  consumed_at TEXT,
  revoked_at TEXT,
  attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
  bootstrap_owner INTEGER NOT NULL DEFAULT 0 CHECK (bootstrap_owner IN (0, 1))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_account_invitations_live_login
ON account_invitations(canonical_login)
WHERE consumed_at IS NULL AND revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_account_invitations_expiry
ON account_invitations(expires_at);

CREATE TABLE IF NOT EXISTS account_activation_challenges (
  id TEXT PRIMARY KEY,
  invitation_id TEXT NOT NULL REFERENCES account_invitations(id) ON DELETE CASCADE,
  challenge_hash TEXT NOT NULL UNIQUE CHECK (length(challenge_hash) = 64),
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  consumed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_account_activation_challenges_invitation
ON account_activation_challenges(invitation_id);

CREATE TABLE IF NOT EXISTS owner_bootstrap_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  consumed INTEGER NOT NULL CHECK (consumed IN (0, 1))
);

INSERT OR IGNORE INTO owner_bootstrap_state(singleton, consumed) VALUES (1, 0);
