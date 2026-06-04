use std::path::PathBuf;

use chrono::{SecondsFormat, Utc};
use sha2::{Digest, Sha256};

/// Current schema version for the code graph store.
pub const SCHEMA_VERSION: i64 = 1;

pub(crate) const DEFAULT_REPO_ID: &str = "local";
pub(crate) const DEFAULT_OWNER: &str = "local";
pub(crate) const DEFAULT_NAME: &str = "workspace";
pub(crate) const DEFAULT_REF_NAME: &str = "HEAD";
pub(crate) const DEFAULT_COMMIT_SHA: &str = "unknown";
pub(crate) const DEFAULT_ROOT: &str = ".";
pub(crate) const STORAGE_BACKEND: &str = "sqlite";

/// Versioned schema applied to a new store.
pub const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS codegraph_repos (
    repo_id TEXT PRIMARY KEY,
    owner   TEXT NOT NULL,
    name    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_schema_migrations (
    version      INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    applied_at   TEXT NOT NULL,
    schema_digest TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_index_runs (
    run_id              TEXT PRIMARY KEY,
    repo_id             TEXT NOT NULL REFERENCES codegraph_repos(repo_id) ON DELETE CASCADE,
    ref_name            TEXT NOT NULL,
    commit_sha          TEXT NOT NULL,
    root                TEXT NOT NULL,
    indexed_at          TEXT NOT NULL,
    analyzer_scope_json TEXT NOT NULL CHECK (json_valid(analyzer_scope_json)),
    graph_stats_json    TEXT NOT NULL CHECK (json_valid(graph_stats_json)),
    schema_version      INTEGER NOT NULL,
    schema_digest       TEXT NOT NULL,
    cache_status        TEXT NOT NULL CHECK (cache_status IN ('hit', 'refreshed'))
);

CREATE INDEX IF NOT EXISTS codegraph_index_runs_repo_commit_idx
    ON codegraph_index_runs(repo_id, commit_sha);
CREATE INDEX IF NOT EXISTS codegraph_index_runs_repo_ref_idx
    ON codegraph_index_runs(repo_id, ref_name);
CREATE INDEX IF NOT EXISTS codegraph_index_runs_scope_idx
    ON codegraph_index_runs(repo_id, commit_sha, schema_version, analyzer_scope_json);

CREATE TABLE IF NOT EXISTS codegraph_symbols (
    repo_id    TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    crate      TEXT NOT NULL,
    file       TEXT NOT NULL,
    symbol     TEXT NOT NULL,
    kind       TEXT NOT NULL,
    is_public  INTEGER NOT NULL CHECK (is_public IN (0, 1)),
    line       INTEGER NOT NULL,
    PRIMARY KEY (repo_id, commit_sha, crate, file, symbol),
    FOREIGN KEY (repo_id) REFERENCES codegraph_repos(repo_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS codegraph_symbols_repo_commit_crate_idx
    ON codegraph_symbols(repo_id, commit_sha, crate);

CREATE TABLE IF NOT EXISTS codegraph_crate_deps (
    repo_id    TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    crate      TEXT NOT NULL,
    depends_on TEXT NOT NULL,
    PRIMARY KEY (repo_id, commit_sha, crate, depends_on),
    FOREIGN KEY (repo_id) REFERENCES codegraph_repos(repo_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS codegraph_crate_deps_repo_commit_idx
    ON codegraph_crate_deps(repo_id, commit_sha);

CREATE TABLE IF NOT EXISTS codegraph_files (
    repo_id             TEXT NOT NULL,
    commit_sha          TEXT NOT NULL,
    path                TEXT NOT NULL,
    crate               TEXT NOT NULL,
    language            TEXT NOT NULL,
    owner               TEXT NOT NULL,
    test_lane           TEXT NOT NULL,
    proof_lanes_json    TEXT NOT NULL CHECK (json_valid(proof_lanes_json)),
    generated_zone      INTEGER NOT NULL CHECK (generated_zone IN (0, 1)),
    editable            INTEGER NOT NULL CHECK (editable IN (0, 1)),
    provenance_json     TEXT NOT NULL CHECK (json_valid(provenance_json)),
    PRIMARY KEY (repo_id, commit_sha, path),
    FOREIGN KEY (repo_id) REFERENCES codegraph_repos(repo_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS codegraph_files_repo_commit_idx
    ON codegraph_files(repo_id, commit_sha);

CREATE TABLE IF NOT EXISTS codegraph_governance (
    repo_id    TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    path       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    digest     TEXT NOT NULL,
    loaded     INTEGER NOT NULL CHECK (loaded IN (0, 1)),
    PRIMARY KEY (repo_id, commit_sha, path, kind),
    FOREIGN KEY (repo_id) REFERENCES codegraph_repos(repo_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS codegraph_governance_repo_commit_idx
    ON codegraph_governance(repo_id, commit_sha);

CREATE TABLE IF NOT EXISTS codegraph_slice_locks (
    id           TEXT PRIMARY KEY,
    crate        TEXT NOT NULL,
    prefixes_json TEXT NOT NULL,
    locked_by    TEXT NOT NULL,
    reason       TEXT NOT NULL,
    locked_at    TEXT NOT NULL,
    expires_at   TEXT
);

CREATE TABLE IF NOT EXISTS codegraph_outbox (
    event_id      TEXT PRIMARY KEY,
    event_type    TEXT NOT NULL,
    repo_id       TEXT NOT NULL,
    ref_name      TEXT NOT NULL,
    commit_sha    TEXT NOT NULL,
    run_id        TEXT NOT NULL,
    payload_json  TEXT NOT NULL CHECK (json_valid(payload_json)),
    created_at    TEXT NOT NULL,
    delivered_at  TEXT
);

CREATE INDEX IF NOT EXISTS codegraph_outbox_repo_commit_idx
    ON codegraph_outbox(repo_id, commit_sha);
"#;

/// Default database location under the user's `~/.jeryu/` directory.
#[must_use]
pub fn default_db_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".jeryu").join("codegraph.sqlite")
}

/// Digest for the active schema.
#[must_use]
pub fn schema_digest() -> String {
    digest_sha256(SCHEMA)
}

pub(crate) fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn digest_sha256(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}
