use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::error::{CodeGraphError, Result};

use super::CodeGraphStore;
use super::schema::{DEFAULT_COMMIT_SHA, SCHEMA_VERSION, STORAGE_BACKEND};
use super::types::{
    CacheStatus, CrateDepRow, FileRecord, GovernanceRecord, GraphSnapshot, GraphStats,
    IndexReceipt, QueryOptions, RepoIdentity, SymbolRow,
};

impl CodeGraphStore {
    /// Loads the default snapshot back from storage.
    pub fn load_snapshot(&self) -> Result<GraphSnapshot> {
        let repo = RepoIdentity::default();
        let scope = QueryOptions::default();
        self.load_snapshot_for_scope(&repo, DEFAULT_COMMIT_SHA, &scope)
    }

    /// Loads the latest snapshot for a repository/commit and query scope.
    pub fn load_snapshot_for_scope(
        &self,
        repo: &RepoIdentity,
        commit_sha: &str,
        query_options: &QueryOptions,
    ) -> Result<GraphSnapshot> {
        query_options.validate()?;
        let conn = self.connect()?;
        let Some(receipt) = self.latest_receipt(&conn, repo, commit_sha, query_options)? else {
            return Ok(GraphSnapshot::default());
        };
        self.load_snapshot_for_receipt(&conn, &receipt)
    }

    /// Returns the latest receipt for a repository/commit/scope if present.
    pub fn latest_index_receipt(
        &self,
        repo: &RepoIdentity,
        commit_sha: &str,
        query_options: &QueryOptions,
    ) -> Result<Option<IndexReceipt>> {
        query_options.validate()?;
        let conn = self.connect()?;
        self.latest_receipt(&conn, repo, commit_sha, query_options)
    }

    /// Returns either the cached snapshot or a freshly built one.
    pub fn query_or_refresh<F>(
        &self,
        repo: &RepoIdentity,
        ref_name: &str,
        commit_sha: &str,
        root: impl AsRef<std::path::Path>,
        query_options: &QueryOptions,
        build_snapshot: F,
    ) -> Result<(GraphSnapshot, IndexReceipt)>
    where
        F: FnOnce() -> Result<GraphSnapshot>,
    {
        query_options.validate()?;
        if let Some(receipt) = self.latest_index_receipt(repo, commit_sha, query_options)? {
            let conn = self.connect()?;
            let snapshot = self.load_snapshot_for_receipt(&conn, &receipt)?;
            let mut hit = receipt;
            hit.cache_status = CacheStatus::Hit;
            hit.outbox_event_id = None;
            return Ok((snapshot, hit));
        }

        let snapshot = build_snapshot()?;
        let receipt =
            self.persist_repo_commit(repo, ref_name, commit_sha, root, query_options, &snapshot)?;
        Ok((snapshot, receipt))
    }

    /// Loads file metadata rows for a repository/commit scope.
    pub fn load_file_records(
        &self,
        repo: &RepoIdentity,
        commit_sha: &str,
    ) -> Result<Vec<FileRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT repo_id, commit_sha, path, crate, language, owner, test_lane, \
                        proof_lanes_json, generated_zone, editable, provenance_json \
                 FROM codegraph_files \
                 WHERE repo_id = ?1 AND commit_sha = ?2 \
                 ORDER BY path",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let mut out = Vec::new();
        let mut rows = stmt
            .query(params![repo.repo_id.as_str(), commit_sha])
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?
        {
            let proof_lanes_json: String = row
                .get(7)
                .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
            let proof_lanes: Vec<String> = serde_json::from_str(&proof_lanes_json)
                .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
            let provenance_json: String = row
                .get(10)
                .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
            let provenance: Value = serde_json::from_str(&provenance_json)
                .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
            out.push(FileRecord {
                repo: repo.clone(),
                commit_sha: row
                    .get(1)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                path: row
                    .get(2)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                crate_name: row
                    .get(3)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                language: row
                    .get(4)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                owner: row
                    .get(5)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                test_lane: row
                    .get(6)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                proof_lanes,
                generated_zone: row
                    .get::<_, i64>(8)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?
                    != 0,
                editable: row
                    .get::<_, i64>(9)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?
                    != 0,
                provenance,
            });
        }
        Ok(out)
    }

    /// Loads governance metadata rows for a repository/commit scope.
    pub fn load_governance_records(
        &self,
        repo: &RepoIdentity,
        commit_sha: &str,
    ) -> Result<Vec<GovernanceRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                "SELECT repo_id, commit_sha, path, kind, digest, loaded \
                 FROM codegraph_governance \
                 WHERE repo_id = ?1 AND commit_sha = ?2 \
                 ORDER BY path, kind",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let mut out = Vec::new();
        let mut rows = stmt
            .query(params![repo.repo_id.as_str(), commit_sha])
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?
        {
            out.push(GovernanceRecord {
                repo: repo.clone(),
                commit_sha: row
                    .get(1)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                path: row
                    .get(2)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                kind: row
                    .get(3)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                digest: row
                    .get(4)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
                loaded: row
                    .get::<_, i64>(5)
                    .map_err(|e| CodeGraphError::Storage(e.to_string()))?
                    != 0,
            });
        }
        Ok(out)
    }

    fn latest_receipt(
        &self,
        conn: &Connection,
        repo: &RepoIdentity,
        commit_sha: &str,
        query_options: &QueryOptions,
    ) -> Result<Option<IndexReceipt>> {
        let analyzer_scope_json = query_options.canonical_json()?;
        let mut stmt = conn
            .prepare(
                "SELECT run_id, repo_id, ref_name, commit_sha, root, indexed_at, \
                        analyzer_scope_json, graph_stats_json, schema_version, schema_digest, cache_status \
                 FROM codegraph_index_runs \
                 WHERE repo_id = ?1 AND commit_sha = ?2 \
                   AND schema_version = ?3 AND analyzer_scope_json = ?4 \
                 ORDER BY indexed_at DESC, run_id DESC \
                 LIMIT 1",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let receipt = stmt
            .query_row(
                params![
                    repo.repo_id.as_str(),
                    commit_sha,
                    SCHEMA_VERSION,
                    analyzer_scope_json
                ],
                |row| row_to_receipt(row, repo),
            )
            .optional()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(receipt)
    }

    fn load_snapshot_for_receipt(
        &self,
        conn: &Connection,
        receipt: &IndexReceipt,
    ) -> Result<GraphSnapshot> {
        let stats: GraphStats = serde_json::from_str(&receipt.graph_stats_json)
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let mut snapshot = GraphSnapshot::default();

        let mut stmt = conn
            .prepare(
                "SELECT crate, file, symbol, kind, is_public, line \
                 FROM codegraph_symbols \
                 WHERE repo_id = ?1 AND commit_sha = ?2 \
                 ORDER BY crate, file, symbol",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map(
                params![receipt.repo.repo_id.as_str(), receipt.commit_sha.as_str()],
                |row| {
                    Ok(SymbolRow {
                        crate_name: row.get(0)?,
                        file: row.get(1)?,
                        symbol: row.get(2)?,
                        kind: row.get(3)?,
                        is_public: row.get::<_, i64>(4)? != 0,
                        line: row.get::<_, i64>(5)? as u32,
                    })
                },
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in rows {
            snapshot
                .symbols
                .push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
        }

        let mut dep_stmt = conn
            .prepare(
                "SELECT crate, depends_on FROM codegraph_crate_deps \
                 WHERE repo_id = ?1 AND commit_sha = ?2 \
                 ORDER BY crate, depends_on",
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let dep_rows = dep_stmt
            .query_map(
                params![receipt.repo.repo_id.as_str(), receipt.commit_sha.as_str()],
                |row| {
                    Ok(CrateDepRow {
                        crate_name: row.get(0)?,
                        depends_on: row.get(1)?,
                    })
                },
            )
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        for row in dep_rows {
            snapshot
                .crate_deps
                .push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
        }

        if stats.symbols > 0 && snapshot.symbols.is_empty() {
            return Err(CodeGraphError::Storage(
                "missing codegraph symbol rows for indexed receipt".to_string(),
            ));
        }
        if stats.crate_deps > 0 && snapshot.crate_deps.is_empty() {
            return Err(CodeGraphError::Storage(
                "missing codegraph dependency rows for indexed receipt".to_string(),
            ));
        }

        Ok(snapshot)
    }
}

fn row_to_receipt(row: &rusqlite::Row<'_>, repo: &RepoIdentity) -> rusqlite::Result<IndexReceipt> {
    let cache_status: String = row.get(10)?;
    Ok(IndexReceipt {
        run_id: row.get(0)?,
        outbox_event_id: None,
        storage_backend: STORAGE_BACKEND.to_string(),
        repo: RepoIdentity {
            repo_id: row.get(1)?,
            owner: repo.owner.clone(),
            name: repo.name.clone(),
        },
        ref_name: row.get(2)?,
        commit_sha: row.get(3)?,
        root: row.get(4)?,
        indexed_at: row.get(5)?,
        analyzer_scope_json: row.get(6)?,
        graph_stats_json: row.get(7)?,
        schema_version: row.get(8)?,
        schema_digest: row.get(9)?,
        cache_status: match cache_status.as_str() {
            "hit" => CacheStatus::Hit,
            _ => CacheStatus::Refreshed,
        },
    })
}
