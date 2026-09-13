//! Self-contained SQLite storage for the code graph.
//!
//! Schema lives in `db/migrations/0001_codegraph.sql` and is applied on open.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::error::{CodeGraphError, Result};
use crate::tool_build::{ToolBuildCluster, ToolBuildIgnore, ToolBuildScanReport};

mod helpers;
mod persist;
mod types;

use helpers::*;
pub use types::{
    CrateDepRow, FileRow, GovernanceRow, GraphSnapshot, IndexRunRow, SymbolRefRow, SymbolRow,
};

/// Embedded code-graph schema. Applied via `execute_batch` on open.
pub const SCHEMA: &str = include_str!("../../../db/migrations/0001_codegraph.sql");

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
        migrate_tool_build_clusters(&conn)?;
        Ok(store)
    }

    /// Opens the store at the default `~/.jeryu/codegraph.sqlite` path.
    pub fn open_default() -> Result<Self> {
        Self::open(default_db_path())
    }

    pub(crate) fn connect(&self) -> Result<Connection> {
        let conn =
            Connection::open(&self.path).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        // Concurrent readers/writers are normal here: the live API serves the
        // dashboard while a scan persists, and CLI scans run alongside the
        // server. Wait for the lock instead of failing with "database is
        // locked".
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(conn)
    }

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
                  occurrences_json, created_at, category, member_cluster_ids_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                    cluster.category.as_str(),
                    serde_json::to_string(&cluster.member_cluster_ids)
                        .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
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
                       c.category, c.member_cluster_ids_json, \
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

    /// The `created_at` stamp (unix millis) of the persisted scan for
    /// `repo_id`, or `None` when no scan has been persisted.
    pub fn tool_build_scanned_at(&self, repo_id: &str) -> Result<Option<String>> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT MAX(created_at) FROM codegraph_tool_build_clusters WHERE repo_id = ?1",
            params![repo_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))
    }

    /// Group the persisted clusters for `repo_id` into pattern families.
    /// Families are computed on read (a pure function of the cluster rows),
    /// so they can never go stale against the persisted clusters.
    pub fn tool_build_families(
        &self,
        repo_id: Option<&str>,
        limit: usize,
        include_ignored: bool,
    ) -> Result<Vec<crate::tool_build::ToolBuildClusterFamily>> {
        let clusters = self.tool_build_clusters(repo_id, limit, include_ignored)?;
        Ok(crate::tool_build::group_pattern_families(&clusters))
    }

    /// Inherit ignore feedback recorded against pre-merge window cluster ids
    /// onto the merged clusters that absorbed them, so an operator's earlier
    /// "not a lead" verdict survives overlap merging. Returns how many merged
    /// clusters gained an inherited ignore.
    pub fn propagate_ignores_to_merged(&self, clusters: &[ToolBuildCluster]) -> Result<usize> {
        let conn = self.connect()?;
        let mut inherited = 0usize;
        for cluster in clusters {
            if cluster.member_cluster_ids.is_empty() || cluster.ignored.is_some() {
                continue;
            }
            for member_id in &cluster.member_cluster_ids {
                if member_id == &cluster.cluster_id {
                    continue;
                }
                let existing: Option<(String, String)> = conn
                    .query_row(
                        "SELECT reason, ignored_by FROM codegraph_tool_build_ignores \
                         WHERE cluster_id = ?1",
                        params![member_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map(Some)
                    .or_else(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(CodeGraphError::Storage(other.to_string())),
                    })?;
                let Some((reason, ignored_by)) = existing else {
                    continue;
                };
                let changed = conn
                    .execute(
                        "INSERT OR IGNORE INTO codegraph_tool_build_ignores \
                         (cluster_id, reason, ignored_by, ignored_at) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            cluster.cluster_id,
                            format!("inherited: {member_id}: {reason}"),
                            ignored_by,
                            epoch_millis(),
                        ],
                    )
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
                if changed > 0 {
                    inherited += 1;
                }
                break;
            }
        }
        Ok(inherited)
    }

    /// Returns the on-disk path of this store.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}
