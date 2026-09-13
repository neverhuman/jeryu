-- Independent append/terminal persistence; no catalog foreign keys or State
-- snapshot ownership. Retain these records during catalog deletion/recovery.
CREATE TABLE IF NOT EXISTS forge_required_attempts (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    context TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal > 0),
    idempotency_key TEXT NOT NULL,
    reservation_json TEXT NOT NULL CHECK (json_valid(reservation_json)),
    attempt_json TEXT NOT NULL CHECK (json_valid(attempt_json)),
    completed INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1)),
    UNIQUE (repo_id, idempotency_key),
    UNIQUE (repo_id, commit_sha, context, ordinal)
);

CREATE INDEX IF NOT EXISTS forge_required_attempt_latest
ON forge_required_attempts(repo_id, commit_sha, context, ordinal DESC);

CREATE TABLE IF NOT EXISTS forge_required_attempt_artifacts (
    attempt_id TEXT NOT NULL,
    name TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
    content BLOB NOT NULL,
    PRIMARY KEY (attempt_id, name),
    CHECK (length(content) = size_bytes)
);

CREATE TABLE IF NOT EXISTS forge_required_attempt_outbox (
    id TEXT PRIMARY KEY NOT NULL,
    attempt_id TEXT NOT NULL UNIQUE,
    repo_id TEXT NOT NULL,
    event_json TEXT NOT NULL CHECK (json_valid(event_json)),
    delivered_at TEXT,
    delivery_receipt TEXT
);
