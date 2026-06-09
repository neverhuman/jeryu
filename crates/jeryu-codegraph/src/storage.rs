//! Self-contained SQLite storage for the code graph.
//!
//! Mirrors the `SqliteStore` pattern in `jeryu-core::engine::storage`
//! (open -> apply schema via `execute_batch` -> atomic snapshot persist via a
//! transaction) but is fully self-contained: this crate opens its own
//! `codegraph.sqlite` and applies its own embedded schema. It never touches the
//! shared `db/migrations/` set and never edits `jeryu-core`.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params, types::Type};
use serde::{Deserialize, Serialize};

use crate::error::{CodeGraphError, Result};
use crate::tool_build::{ToolBuildCluster, ToolBuildIgnore, ToolBuildScanReport};

/// Embedded code-graph schema. Applied via `execute_batch` on open.
pub const SCHEMA: &str = r#"
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
    cluster_id         TEXT PRIMARY KEY,
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
    created_at         TEXT NOT NULL
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

INSERT OR REPLACE INTO codegraph_meta (key, value) VALUES ('schema_version', '3');
"#;

/// Default database location under the user's local Jeryu data directory.
#[must_use]
pub fn default_db_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local")
        .join("share")
        .join("jeryu")
        .join("codegraph.sqlite")
}

/// A row in `codegraph_symbols`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolRow {
    /// Owning crate (workspace package name).
    pub crate_name: String,
    /// Repo-relative source file path.
    pub file: String,
    /// Symbol name.
    pub symbol: String,
    /// Symbol kind (e.g. `public`).
    pub kind: String,
    /// Whether the symbol is part of the public API.
    pub is_public: bool,
    /// 1-based line number (0 when unknown).
    pub line: u32,
}

/// A row in `codegraph_crate_deps`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateDepRow {
    /// Dependent crate.
    pub crate_name: String,
    /// Crate it depends on.
    pub depends_on: String,
}

/// A row in `codegraph_symbol_refs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolRefRow {
    /// Owning crate for the referenced symbol.
    pub crate_name: String,
    /// Definition file for the referenced symbol.
    pub file: String,
    /// Referenced symbol name.
    pub symbol: String,
    /// Repo-relative file containing the reference.
    pub ref_file: String,
    /// 1-based reference line number (0 when unknown).
    pub ref_line: u32,
    /// Reference kind, for example `call`, `type`, or `mention`.
    pub ref_kind: String,
}

/// A repo file recorded with governance and provenance metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRow {
    /// Stable repository id.
    pub repo_id: String,
    /// Commit sha the row was indexed from.
    pub commit_sha: String,
    /// Repo-relative file path.
    pub path: String,
    /// Owning Rust crate when known.
    pub crate_name: Option<String>,
    /// Analyzer language/domain label.
    pub language: String,
    /// Owner-map owner when known.
    pub owner: Option<String>,
    /// Test-map lane when known.
    pub test_lane: Option<String>,
    /// Proof lanes attached to the file.
    pub proof_lanes: Vec<String>,
    /// Matching generated-zone path when the file is generated.
    pub generated_zone: Option<String>,
    /// Whether an agent should treat the file as directly editable.
    pub editable: bool,
    /// JSON provenance records for this row.
    pub provenance_json: String,
}

/// A loaded governance metadata file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceRow {
    /// Stable repository id.
    pub repo_id: String,
    /// Commit sha the row was loaded from.
    pub commit_sha: String,
    /// Repo-relative governance path.
    pub path: String,
    /// Governance kind, such as `owner_map` or `proof_lanes`.
    pub kind: String,
    /// Lightweight content digest for provenance.
    pub digest: String,
    /// Whether the file was present and loaded.
    pub loaded: bool,
}

/// Receipt for one index refresh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRunRow {
    /// Stable run id for this refresh.
    pub run_id: String,
    /// Stable repository id.
    pub repo_id: String,
    /// Ref name requested by the caller.
    pub ref_name: String,
    /// Commit sha indexed.
    pub commit_sha: String,
    /// Materialized root that was indexed.
    pub root: String,
    /// Timestamp for the refresh.
    pub indexed_at: String,
    /// JSON array of enabled analyzers.
    pub analyzer_scope_json: String,
    /// JSON object with graph counts.
    pub graph_stats_json: String,
}

/// A persistable snapshot of the code graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    /// All indexed symbol rows.
    pub symbols: Vec<SymbolRow>,
    /// All recorded crate dependency edges.
    pub crate_deps: Vec<CrateDepRow>,
    /// All recorded symbol reference rows.
    pub symbol_refs: Vec<SymbolRefRow>,
    /// Files with attached governance metadata.
    pub files: Vec<FileRow>,
    /// Governance metadata files loaded for this snapshot.
    pub governance: Vec<GovernanceRow>,
    /// Index refresh receipts.
    pub index_runs: Vec<IndexRunRow>,
}

/// Self-contained SQLite store for the code graph.
#[derive(Debug, Clone)]
pub struct CodeGraphStore {
    path: PathBuf,
}

impl CodeGraphStore {
    /// Opens (creating if needed) the store at `path` and applies the embedded
    /// schema. Mirrors `SqliteStore::open`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        let store = Self { path };
        let conn = store.connect()?;
        conn.execute_batch(SCHEMA)
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(store)
    }

    /// Opens the store at the default `~/.jeryu/codegraph.sqlite` path.
    pub fn open_default() -> Result<Self> {
        Self::open(default_db_path())
    }

    fn connect(&self) -> Result<Connection> {
        let conn =
            Connection::open(&self.path).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(conn)
    }

    /// Persists a full snapshot atomically, mirroring `SqliteStore::persist`
    /// (delete-all then re-insert inside a single transaction).
    pub fn persist(&self, snapshot: &GraphSnapshot) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        tx.execute_batch(
            "DELETE FROM codegraph_symbols; \
             DELETE FROM codegraph_crate_deps; \
             DELETE FROM codegraph_symbol_refs; \
             DELETE FROM codegraph_files; \
             DELETE FROM codegraph_governance; \
             DELETE FROM codegraph_index_runs;",
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in &snapshot.symbols {
            tx.execute(
                "INSERT INTO codegraph_symbols (crate, file, symbol, kind, is_public, line) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    row.crate_name,
                    row.file,
                    row.symbol,
                    row.kind,
                    i64::from(row.is_public),
                    i64::from(row.line),
                ],
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        for dep in &snapshot.crate_deps {
            tx.execute(
                "INSERT INTO codegraph_crate_deps (crate, depends_on) VALUES (?1, ?2)",
                params![dep.crate_name, dep.depends_on],
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        for reference in &snapshot.symbol_refs {
            tx.execute(
                "INSERT INTO codegraph_symbol_refs \
                 (crate, file, symbol, ref_file, ref_line, ref_kind) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    reference.crate_name,
                    reference.file,
                    reference.symbol,
                    reference.ref_file,
                    i64::from(reference.ref_line),
                    reference.ref_kind,
                ],
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        for file in &snapshot.files {
            tx.execute(
                "INSERT INTO codegraph_files \
                 (repo_id, commit_sha, path, crate, language, owner, test_lane, \
                  proof_lanes_json, generated_zone, editable, provenance_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    file.repo_id,
                    file.commit_sha,
                    file.path,
                    file.crate_name,
                    file.language,
                    file.owner,
                    file.test_lane,
                    serde_json::to_string(&file.proof_lanes)
                        .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                    file.generated_zone,
                    i64::from(file.editable),
                    file.provenance_json,
                ],
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        for row in &snapshot.governance {
            tx.execute(
                "INSERT INTO codegraph_governance \
                 (repo_id, commit_sha, path, kind, digest, loaded) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    row.repo_id,
                    row.commit_sha,
                    row.path,
                    row.kind,
                    row.digest,
                    i64::from(row.loaded),
                ],
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        for row in &snapshot.index_runs {
            tx.execute(
                "INSERT INTO codegraph_index_runs \
                 (run_id, repo_id, ref_name, commit_sha, root, indexed_at, \
                  analyzer_scope_json, graph_stats_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    row.run_id,
                    row.repo_id,
                    row.ref_name,
                    row.commit_sha,
                    row.root,
                    row.indexed_at,
                    row.analyzer_scope_json,
                    row.graph_stats_json,
                ],
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        tx.commit()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Loads the full snapshot back from storage.
    pub fn load_snapshot(&self) -> Result<GraphSnapshot> {
        let conn = self.connect()?;
        let mut snapshot = GraphSnapshot::default();

        let mut stmt = conn
            .prepare(
                "SELECT crate, file, symbol, kind, is_public, line \
                 FROM codegraph_symbols ORDER BY crate, file, symbol",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SymbolRow {
                    crate_name: row.get(0)?,
                    file: row.get(1)?,
                    symbol: row.get(2)?,
                    kind: row.get(3)?,
                    is_public: row.get::<_, i64>(4)? != 0,
                    line: row.get::<_, i64>(5)? as u32,
                })
            })
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in rows {
            snapshot
                .symbols
                .push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
        }

        let mut dep_stmt = conn
            .prepare(
                "SELECT crate, depends_on FROM codegraph_crate_deps \
                 ORDER BY crate, depends_on",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let dep_rows = dep_stmt
            .query_map([], |row| {
                Ok(CrateDepRow {
                    crate_name: row.get(0)?,
                    depends_on: row.get(1)?,
                })
            })
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in dep_rows {
            snapshot
                .crate_deps
                .push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
        }

        let mut ref_stmt = conn
            .prepare(
                "SELECT crate, file, symbol, ref_file, ref_line, ref_kind \
                 FROM codegraph_symbol_refs ORDER BY crate, symbol, ref_file, ref_line",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let ref_rows = ref_stmt
            .query_map([], |row| {
                Ok(SymbolRefRow {
                    crate_name: row.get(0)?,
                    file: row.get(1)?,
                    symbol: row.get(2)?,
                    ref_file: row.get(3)?,
                    ref_line: row.get::<_, i64>(4)? as u32,
                    ref_kind: row.get(5)?,
                })
            })
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in ref_rows {
            snapshot
                .symbol_refs
                .push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
        }

        let mut file_stmt = conn
            .prepare(
                "SELECT repo_id, commit_sha, path, crate, language, owner, test_lane, \
                 proof_lanes_json, generated_zone, editable, provenance_json \
                 FROM codegraph_files ORDER BY path",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let file_rows = file_stmt
            .query_map([], |row| {
                let proof_lanes_json: String = row.get(7)?;
                let proof_lanes: Vec<String> = serde_json::from_str(&proof_lanes_json)
                    .map_err(|error| sqlite_json_error(7, error))?;
                Ok(FileRow {
                    repo_id: row.get(0)?,
                    commit_sha: row.get(1)?,
                    path: row.get(2)?,
                    crate_name: row.get(3)?,
                    language: row.get(4)?,
                    owner: row.get(5)?,
                    test_lane: row.get(6)?,
                    proof_lanes,
                    generated_zone: row.get(8)?,
                    editable: row.get::<_, i64>(9)? != 0,
                    provenance_json: row.get(10)?,
                })
            })
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in file_rows {
            snapshot
                .files
                .push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
        }

        let mut gov_stmt = conn
            .prepare(
                "SELECT repo_id, commit_sha, path, kind, digest, loaded \
                 FROM codegraph_governance ORDER BY path",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let gov_rows = gov_stmt
            .query_map([], |row| {
                Ok(GovernanceRow {
                    repo_id: row.get(0)?,
                    commit_sha: row.get(1)?,
                    path: row.get(2)?,
                    kind: row.get(3)?,
                    digest: row.get(4)?,
                    loaded: row.get::<_, i64>(5)? != 0,
                })
            })
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in gov_rows {
            snapshot
                .governance
                .push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
        }

        let mut run_stmt = conn
            .prepare(
                "SELECT run_id, repo_id, ref_name, commit_sha, root, indexed_at, \
                 analyzer_scope_json, graph_stats_json \
                 FROM codegraph_index_runs ORDER BY indexed_at, run_id",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let run_rows = run_stmt
            .query_map([], |row| {
                Ok(IndexRunRow {
                    run_id: row.get(0)?,
                    repo_id: row.get(1)?,
                    ref_name: row.get(2)?,
                    commit_sha: row.get(3)?,
                    root: row.get(4)?,
                    indexed_at: row.get(5)?,
                    analyzer_scope_json: row.get(6)?,
                    graph_stats_json: row.get(7)?,
                })
            })
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in run_rows {
            snapshot
                .index_runs
                .push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
        }

        Ok(snapshot)
    }

    /// Search persisted symbols by substring, ordered deterministically.
    pub fn search_symbols(&self, query: &str, limit: usize) -> Result<Vec<SymbolRow>> {
        let conn = self.connect()?;
        let pattern = format!("%{query}%");
        let mut stmt = conn
            .prepare(
                "SELECT crate, file, symbol, kind, is_public, line \
                 FROM codegraph_symbols \
                 WHERE symbol LIKE ?1 OR file LIKE ?1 OR crate LIKE ?1 \
                 ORDER BY crate, file, symbol LIMIT ?2",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(params![pattern, limit.max(1) as i64], |row| {
                Ok(SymbolRow {
                    crate_name: row.get(0)?,
                    file: row.get(1)?,
                    symbol: row.get(2)?,
                    kind: row.get(3)?,
                    is_public: row.get::<_, i64>(4)? != 0,
                    line: row.get::<_, i64>(5)? as u32,
                })
            })
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        collect_rows(rows)
    }

    /// Return the first persisted definition row for `symbol`.
    pub fn definition(&self, symbol: &str) -> Result<Option<SymbolRow>> {
        Ok(self
            .search_symbols(symbol, 100)?
            .into_iter()
            .find(|row| row.symbol == symbol))
    }

    /// Return all persisted references for `symbol`.
    pub fn references(&self, symbol: &str) -> Result<Vec<SymbolRefRow>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT crate, file, symbol, ref_file, ref_line, ref_kind \
                 FROM codegraph_symbol_refs WHERE symbol = ?1 \
                 ORDER BY crate, symbol, ref_file, ref_line",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(params![symbol], |row| {
                Ok(SymbolRefRow {
                    crate_name: row.get(0)?,
                    file: row.get(1)?,
                    symbol: row.get(2)?,
                    ref_file: row.get(3)?,
                    ref_line: row.get::<_, i64>(4)? as u32,
                    ref_kind: row.get(5)?,
                })
            })
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        collect_rows(rows)
    }

    /// Return crates that directly depend on `crate_name`.
    pub fn reverse_deps(&self, crate_name: &str) -> Result<Vec<String>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT crate FROM codegraph_crate_deps WHERE depends_on = ?1 ORDER BY crate")
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(params![crate_name], |row| row.get::<_, String>(0))
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        collect_rows(rows)
    }

    /// Return the embedded schema version recorded in `codegraph_meta`.
    pub fn schema_version(&self) -> Result<String> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT value FROM codegraph_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))
    }

    /// Persist the ranked tool-building clusters from a fast scan.
    pub fn persist_tool_build_report(&self, report: &ToolBuildScanReport) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        tx.execute(
            "DELETE FROM codegraph_tool_build_clusters WHERE repo_id = ?1 AND commit_sha = ?2",
            params![report.repo_id, report.commit_sha],
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for cluster in &report.clusters {
            tx.execute(
                "INSERT OR REPLACE INTO codegraph_tool_build_clusters \
                 (cluster_id, repo_id, commit_sha, fingerprint, score, occurrence_count, \
                  repo_count, file_count, total_lines, language, insight, normalized_preview, \
                  occurrences_json, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    cluster.cluster_id,
                    cluster.repo_id,
                    cluster.commit_sha,
                    cluster.fingerprint,
                    i64::try_from(cluster.score).unwrap_or(i64::MAX),
                    i64::try_from(cluster.occurrence_count).unwrap_or(i64::MAX),
                    i64::try_from(cluster.repo_count).unwrap_or(i64::MAX),
                    i64::try_from(cluster.file_count).unwrap_or(i64::MAX),
                    i64::try_from(cluster.total_lines).unwrap_or(i64::MAX),
                    cluster.language,
                    cluster.insight,
                    cluster.normalized_preview,
                    serde_json::to_string(&cluster.occurrences)
                        .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                    report.scanned_at,
                ],
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        tx.commit()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Return ranked tool-building clusters. Ignored clusters are excluded by default.
    pub fn tool_build_clusters(
        &self,
        repo_id: Option<&str>,
        limit: usize,
        include_ignored: bool,
    ) -> Result<Vec<ToolBuildCluster>> {
        let conn = self.connect()?;
        let limit = limit.max(1);
        let sql_all = "SELECT c.cluster_id, c.repo_id, c.commit_sha, c.fingerprint, c.score, \
                       c.occurrence_count, c.repo_count, c.file_count, c.total_lines, \
                       c.language, c.insight, c.normalized_preview, c.occurrences_json, \
                       i.reason, i.ignored_by, i.ignored_at \
                       FROM codegraph_tool_build_clusters c \
                       LEFT JOIN codegraph_tool_build_ignores i ON i.cluster_id = c.cluster_id";
        let order = " ORDER BY c.score DESC, c.occurrence_count DESC, c.cluster_id LIMIT ?";
        let rows = match (repo_id, include_ignored) {
            (Some(repo_id), true) => {
                let mut stmt = conn
                    .prepare(&format!("{sql_all} WHERE c.repo_id = ?{order}"))
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![repo_id, i64::try_from(limit).unwrap_or(i64::MAX)],
                        tool_build_cluster_from_row,
                    )
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                collect_rows(rows)?
            }
            (Some(repo_id), false) => {
                let mut stmt = conn
                    .prepare(&format!(
                        "{sql_all} WHERE c.repo_id = ? AND i.cluster_id IS NULL{order}"
                    ))
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![repo_id, i64::try_from(limit).unwrap_or(i64::MAX)],
                        tool_build_cluster_from_row,
                    )
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                collect_rows(rows)?
            }
            (None, true) => {
                let mut stmt = conn
                    .prepare(&format!("{sql_all}{order}"))
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![i64::try_from(limit).unwrap_or(i64::MAX)],
                        tool_build_cluster_from_row,
                    )
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                collect_rows(rows)?
            }
            (None, false) => {
                let mut stmt = conn
                    .prepare(&format!("{sql_all} WHERE i.cluster_id IS NULL{order}"))
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![i64::try_from(limit).unwrap_or(i64::MAX)],
                        tool_build_cluster_from_row,
                    )
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                collect_rows(rows)?
            }
        };
        Ok(rows)
    }

    /// Return `(total_clusters, ignored_clusters)` for the tool-building index.
    pub fn tool_build_cluster_counts(&self, repo_id: Option<&str>) -> Result<(usize, usize)> {
        let conn = self.connect()?;
        let (total, ignored): (i64, i64) = if let Some(repo_id) = repo_id {
            conn.query_row(
                "SELECT COUNT(*), COUNT(i.cluster_id) \
                 FROM codegraph_tool_build_clusters c \
                 LEFT JOIN codegraph_tool_build_ignores i ON i.cluster_id = c.cluster_id \
                 WHERE c.repo_id = ?1",
                params![repo_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        } else {
            conn.query_row(
                "SELECT COUNT(*), COUNT(i.cluster_id) \
                 FROM codegraph_tool_build_clusters c \
                 LEFT JOIN codegraph_tool_build_ignores i ON i.cluster_id = c.cluster_id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        }
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok((total as usize, ignored as usize))
    }

    /// Record durable feedback that a tool-building cluster should be ignored.
    pub fn ignore_tool_build_cluster(
        &self,
        cluster_id: &str,
        reason: &str,
        ignored_by: &str,
    ) -> Result<ToolBuildIgnore> {
        let ignored = ToolBuildIgnore {
            cluster_id: cluster_id.to_string(),
            reason: reason.to_string(),
            ignored_by: ignored_by.to_string(),
            ignored_at: epoch_millis(),
        };
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO codegraph_tool_build_ignores \
             (cluster_id, reason, ignored_by, ignored_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                ignored.cluster_id,
                ignored.reason,
                ignored.ignored_by,
                ignored.ignored_at,
            ],
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(ignored)
    }

    /// Returns the on-disk path of this store.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn collect_rows<T, F>(rows: rusqlite::MappedRows<'_, F>) -> Result<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
    }
    Ok(out)
}

fn tool_build_cluster_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolBuildCluster> {
    let occurrences_json: String = row.get(12)?;
    let occurrences =
        serde_json::from_str(&occurrences_json).map_err(|error| sqlite_json_error(12, error))?;
    let reason: Option<String> = row.get(13)?;
    let ignored_by: Option<String> = row.get(14)?;
    let ignored_at: Option<String> = row.get(15)?;
    let cluster_id: String = row.get(0)?;
    let ignored = match (reason, ignored_by, ignored_at) {
        (Some(reason), Some(ignored_by), Some(ignored_at)) => Some(ToolBuildIgnore {
            cluster_id: cluster_id.clone(),
            reason,
            ignored_by,
            ignored_at,
        }),
        _ => None,
    };
    Ok(ToolBuildCluster {
        cluster_id,
        repo_id: row.get(1)?,
        commit_sha: row.get(2)?,
        fingerprint: row.get(3)?,
        score: row.get::<_, i64>(4)? as u64,
        occurrence_count: row.get::<_, i64>(5)? as usize,
        repo_count: row.get::<_, i64>(6)? as usize,
        file_count: row.get::<_, i64>(7)? as usize,
        total_lines: row.get::<_, i64>(8)? as usize,
        language: row.get(9)?,
        insight: row.get(10)?,
        normalized_preview: row.get(11)?,
        occurrences,
        ignored,
    })
}

fn sqlite_json_error(column: usize, error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
}

fn epoch_millis() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
