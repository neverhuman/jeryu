-- Independently persisted source-review custody. No catalog FK and no inferred
-- authentication backfill: every pre-existing reviews row remains advisory.
CREATE TABLE IF NOT EXISTS forge_review_challenges (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT NOT NULL,
    pull_id TEXT NOT NULL,
    pull_number INTEGER NOT NULL CHECK (pull_number > 0),
    expires_at TEXT NOT NULL,
    challenge_json TEXT NOT NULL CHECK (json_valid(challenge_json)),
    accepted_event_id TEXT UNIQUE
);

CREATE TABLE IF NOT EXISTS forge_bound_review_events (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT NOT NULL,
    pull_id TEXT NOT NULL,
    pull_number INTEGER NOT NULL CHECK (pull_number > 0),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    challenge_id TEXT NOT NULL UNIQUE REFERENCES forge_review_challenges(id),
    event_json TEXT NOT NULL CHECK (json_valid(event_json)),
    UNIQUE (repo_id, pull_id, sequence)
);
CREATE INDEX IF NOT EXISTS idx_bound_reviews_pull
    ON forge_bound_review_events(repo_id, pull_id, sequence);

CREATE TRIGGER IF NOT EXISTS bound_reviews_no_update
BEFORE UPDATE ON forge_bound_review_events BEGIN
    SELECT RAISE(ABORT, 'bound review events are immutable');
END;
CREATE TRIGGER IF NOT EXISTS bound_reviews_no_delete
BEFORE DELETE ON forge_bound_review_events BEGIN
    SELECT RAISE(ABORT, 'bound review events are immutable');
END;
CREATE TRIGGER IF NOT EXISTS review_challenges_no_rebind
BEFORE UPDATE ON forge_review_challenges
WHEN OLD.id IS NOT NEW.id OR OLD.repo_id IS NOT NEW.repo_id
  OR OLD.pull_id IS NOT NEW.pull_id OR OLD.pull_number IS NOT NEW.pull_number
  OR OLD.expires_at IS NOT NEW.expires_at OR OLD.challenge_json IS NOT NEW.challenge_json
  OR OLD.accepted_event_id IS NOT NULL OR NEW.accepted_event_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'review challenge binding is immutable and consumable once');
END;
CREATE TRIGGER IF NOT EXISTS review_challenges_no_delete
BEFORE DELETE ON forge_review_challenges BEGIN
    SELECT RAISE(ABORT, 'review challenge custody is retained');
END;
