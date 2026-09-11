-- NEXT-SLICE PROPOSAL. Enrollment history is never State snapshot-owned or
-- cascade-deleted with catalog accounts/repositories. The root-owned verified
-- commissioning path is the only prospective installer of these records.
CREATE TABLE IF NOT EXISTS forge_required_publishers (
    publisher_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    enrollment_sha256 TEXT NOT NULL UNIQUE CHECK (length(enrollment_sha256) = 64),
    installation_operation_id TEXT NOT NULL UNIQUE,
    record_json TEXT NOT NULL CHECK (json_valid(record_json)),
    revoked INTEGER NOT NULL DEFAULT 0 CHECK (revoked IN (0, 1)),
    PRIMARY KEY (publisher_id, revision)
);
