use rusqlite::params;

use super::helpers::sqlite_json_error;
use super::{
    CodeGraphStore, CrateDepRow, FileRow, GovernanceRow, GraphSnapshot, IndexRunRow, SymbolRefRow,
    SymbolRow,
};
use crate::error::{CodeGraphError, Result};

impl CodeGraphStore {
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
}
