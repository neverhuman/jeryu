use rusqlite::{Transaction, params};
use serde_json::json;

use crate::error::{CodeGraphError, Result};

use super::CodeGraphStore;
use super::schema::{
    DEFAULT_COMMIT_SHA, DEFAULT_REF_NAME, DEFAULT_ROOT, SCHEMA_VERSION, STORAGE_BACKEND,
    now_rfc3339, schema_digest,
};
use super::types::{
    CacheStatus, GraphSnapshot, GraphStats, IndexReceipt, QueryOptions, RepoIdentity,
};

pub(crate) struct PersistRequest<'a> {
    pub(crate) repo: &'a RepoIdentity,
    pub(crate) ref_name: &'a str,
    pub(crate) commit_sha: &'a str,
    pub(crate) root: &'a str,
    pub(crate) query_options: &'a QueryOptions,
    pub(crate) cache_status: CacheStatus,
}

impl CodeGraphStore {
    /// Persists a full snapshot under the default repository scope.
    pub fn persist(&self, snapshot: &GraphSnapshot) -> Result<()> {
        let repo = RepoIdentity::default();
        let scope = QueryOptions::default();
        self.persist_repo_commit(
            &repo,
            DEFAULT_REF_NAME,
            DEFAULT_COMMIT_SHA,
            DEFAULT_ROOT,
            &scope,
            snapshot,
        )?;
        Ok(())
    }

    /// Persists a full snapshot for a specific repository/commit scope.
    pub fn persist_repo_commit(
        &self,
        repo: &RepoIdentity,
        ref_name: &str,
        commit_sha: &str,
        root: impl AsRef<std::path::Path>,
        query_options: &QueryOptions,
        snapshot: &GraphSnapshot,
    ) -> Result<IndexReceipt> {
        query_options.validate()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        let root = root.as_ref().to_string_lossy().into_owned();
        insert_snapshot_rows(
            tx,
            snapshot,
            &PersistRequest {
                repo,
                ref_name,
                commit_sha,
                root: &root,
                query_options,
                cache_status: CacheStatus::Refreshed,
            },
        )
    }
}

pub(crate) fn insert_snapshot_rows(
    tx: Transaction<'_>,
    snapshot: &GraphSnapshot,
    request: &PersistRequest<'_>,
) -> Result<IndexReceipt> {
    let repo_id = request.repo.repo_id.clone();
    let owner = request.repo.owner.clone();
    let name = request.repo.name.clone();
    let commit_sha = request.commit_sha.to_string();
    let ref_name = request.ref_name.to_string();
    let root = request.root.to_string();
    tx.execute(
        "INSERT INTO codegraph_repos (repo_id, owner, name) VALUES (?1, ?2, ?3) \
         ON CONFLICT(repo_id) DO UPDATE SET owner = excluded.owner, name = excluded.name",
        params![&repo_id, &owner, &name],
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;

    tx.execute(
        "DELETE FROM codegraph_symbols WHERE repo_id = ?1 AND commit_sha = ?2",
        params![&repo_id, &commit_sha],
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    tx.execute(
        "DELETE FROM codegraph_crate_deps WHERE repo_id = ?1 AND commit_sha = ?2",
        params![&repo_id, &commit_sha],
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    tx.execute(
        "DELETE FROM codegraph_files WHERE repo_id = ?1 AND commit_sha = ?2",
        params![&repo_id, &commit_sha],
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    tx.execute(
        "DELETE FROM codegraph_governance WHERE repo_id = ?1 AND commit_sha = ?2",
        params![&repo_id, &commit_sha],
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;

    for row in &snapshot.symbols {
        tx.execute(
            "INSERT INTO codegraph_symbols (repo_id, commit_sha, crate, file, symbol, kind, is_public, line) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &repo_id,
                &commit_sha,
                row.crate_name.clone(),
                row.file.clone(),
                row.symbol.clone(),
                row.kind.clone(),
                i64::from(row.is_public),
                i64::from(row.line),
            ],
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    }

    for dep in &snapshot.crate_deps {
        tx.execute(
            "INSERT INTO codegraph_crate_deps (repo_id, commit_sha, crate, depends_on) VALUES (?1, ?2, ?3, ?4)",
            params![
                &repo_id,
                &commit_sha,
                dep.crate_name.clone(),
                dep.depends_on.clone()
            ],
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    }

    let run_id = run_id();
    let analyzer_scope_json = request.query_options.canonical_json()?;
    let graph_stats = GraphStats {
        symbols: snapshot.symbols.len(),
        crate_deps: snapshot.crate_deps.len(),
    };
    let graph_stats_json =
        serde_json::to_string(&graph_stats).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    let indexed_at = now_rfc3339();
    tx.execute(
        "INSERT INTO codegraph_index_runs \
         (run_id, repo_id, ref_name, commit_sha, root, indexed_at, analyzer_scope_json, graph_stats_json, schema_version, schema_digest, cache_status) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            run_id,
            &repo_id,
            &ref_name,
            &commit_sha,
            &root,
            indexed_at,
            analyzer_scope_json.clone(),
            graph_stats_json.clone(),
            SCHEMA_VERSION,
            schema_digest(),
            match request.cache_status {
                CacheStatus::Hit => "hit",
                CacheStatus::Refreshed => "refreshed",
            }
        ],
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;

    let outbox_event_id = if matches!(request.cache_status, CacheStatus::Refreshed) {
        let event_id = event_id();
        let payload = json!({
            "event_type": "codegraph.indexed",
            "repo_id": repo_id,
            "ref_name": ref_name,
            "commit_sha": commit_sha,
            "run_id": run_id,
            "schema_version": SCHEMA_VERSION,
            "schema_digest": schema_digest(),
            "graph_stats": graph_stats,
        });
        let payload_json =
            serde_json::to_string(&payload).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        tx.execute(
            "INSERT INTO codegraph_outbox \
             (event_id, event_type, repo_id, ref_name, commit_sha, run_id, payload_json, created_at, delivered_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
            params![
                event_id,
                "codegraph.indexed",
                &repo_id,
                &ref_name,
                &commit_sha,
                &run_id,
                payload_json,
                indexed_at,
            ],
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Some(event_id)
    } else {
        None
    };

    tx.execute(
        "INSERT OR REPLACE INTO codegraph_meta (key, value) VALUES (?1, ?2)",
        params!["schema_version", SCHEMA_VERSION.to_string()],
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    tx.execute(
        "INSERT OR REPLACE INTO codegraph_meta (key, value) VALUES (?1, ?2)",
        params!["schema_digest", schema_digest()],
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    tx.execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION))
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    tx.commit()
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;

    Ok(IndexReceipt {
        run_id,
        outbox_event_id,
        storage_backend: STORAGE_BACKEND.to_string(),
        repo: request.repo.clone(),
        ref_name,
        commit_sha,
        root,
        indexed_at,
        analyzer_scope_json,
        graph_stats_json,
        schema_version: SCHEMA_VERSION,
        schema_digest: schema_digest(),
        cache_status: request.cache_status,
    })
}

fn event_id() -> String {
    format!(
        "evt-{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    )
}

fn run_id() -> String {
    format!(
        "run-{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    )
}
