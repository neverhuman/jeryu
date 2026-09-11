PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS codegraph_symbols (
    crate     TEXT NOT NULL,
    file      TEXT NOT NULL,
    symbol    TEXT NOT NULL,
    kind      TEXT NOT NULL,
    is_public INTEGER NOT NULL,
    line      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_crate_deps (
    crate      TEXT NOT NULL,
    depends_on TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_symbol_refs (
    crate     TEXT NOT NULL,
    file      TEXT NOT NULL,
    symbol    TEXT NOT NULL,
    ref_file  TEXT NOT NULL,
    ref_line  INTEGER NOT NULL,
    ref_kind  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_files (
    repo_id          TEXT NOT NULL,
    commit_sha       TEXT NOT NULL,
    path             TEXT NOT NULL,
    crate            TEXT,
    language         TEXT NOT NULL,
    owner            TEXT,
    test_lane        TEXT,
    proof_lanes_json TEXT NOT NULL,
    generated_zone   TEXT,
    editable         INTEGER NOT NULL,
    provenance_json  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_governance (
    repo_id    TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    path       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    digest     TEXT NOT NULL,
    loaded     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_index_runs (
    run_id              TEXT PRIMARY KEY,
    repo_id             TEXT NOT NULL,
    ref_name            TEXT NOT NULL,
    commit_sha          TEXT NOT NULL,
    root                TEXT NOT NULL,
    indexed_at          TEXT NOT NULL,
    analyzer_scope_json TEXT NOT NULL,
    graph_stats_json    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_slice_locks (
    id           TEXT PRIMARY KEY,
    crate        TEXT NOT NULL,
    prefixes_json TEXT NOT NULL,
    locked_by    TEXT NOT NULL,
    reason       TEXT NOT NULL,
    locked_at    TEXT NOT NULL,
    expires_at   TEXT
);

CREATE TABLE IF NOT EXISTS codegraph_tool_build_clusters (
    cluster_id         TEXT NOT NULL,
    repo_id            TEXT NOT NULL,
    commit_sha         TEXT NOT NULL,
    fingerprint        TEXT NOT NULL,
    score              INTEGER NOT NULL,
    occurrence_count   INTEGER NOT NULL,
    repo_count         INTEGER NOT NULL,
    file_count         INTEGER NOT NULL,
    total_lines        INTEGER NOT NULL,
    language           TEXT NOT NULL,
    insight            TEXT NOT NULL,
    normalized_preview TEXT NOT NULL,
    occurrences_json   TEXT NOT NULL,
    created_at         TEXT NOT NULL,
    category           TEXT NOT NULL DEFAULT 'tool-candidate',
    member_cluster_ids_json TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (repo_id, cluster_id)
);

CREATE INDEX IF NOT EXISTS idx_codegraph_tool_build_clusters_rank
ON codegraph_tool_build_clusters (repo_id, score DESC, occurrence_count DESC);

CREATE TABLE IF NOT EXISTS codegraph_tool_build_ignores (
    cluster_id TEXT PRIMARY KEY,
    reason     TEXT NOT NULL,
    ignored_by TEXT NOT NULL,
    ignored_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS codegraph_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO codegraph_meta (key, value) VALUES ('schema_version', '4');
