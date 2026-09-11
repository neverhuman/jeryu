use rusqlite::{Connection, types::Type};

use crate::error::{CodeGraphError, Result};
use crate::tool_build::{ToolBuildCluster, ToolBuildIgnore};

/// One-time, shape-detected rebuild of `codegraph_tool_build_clusters` from
/// the v3 `PRIMARY KEY (cluster_id)` shape to the v4 composite
/// `PRIMARY KEY (repo_id, cluster_id)` shape (with category/member columns).
pub(crate) fn migrate_tool_build_clusters(conn: &Connection) -> Result<()> {
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' \
             AND name = 'codegraph_tool_build_clusters'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    if sql.contains("PRIMARY KEY (repo_id, cluster_id)") {
        return Ok(());
    }
    conn.execute_batch(
        r#"
BEGIN;
CREATE TABLE codegraph_tool_build_clusters_v4 (
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
INSERT INTO codegraph_tool_build_clusters_v4
    (cluster_id, repo_id, commit_sha, fingerprint, score, occurrence_count,
     repo_count, file_count, total_lines, language, insight, normalized_preview,
     occurrences_json, created_at)
  SELECT cluster_id, repo_id, commit_sha, fingerprint, score, occurrence_count,
     repo_count, file_count, total_lines, language, insight, normalized_preview,
     occurrences_json, created_at
  FROM codegraph_tool_build_clusters;
DROP TABLE codegraph_tool_build_clusters;
ALTER TABLE codegraph_tool_build_clusters_v4 RENAME TO codegraph_tool_build_clusters;
CREATE INDEX IF NOT EXISTS idx_codegraph_tool_build_clusters_rank
ON codegraph_tool_build_clusters (repo_id, score DESC, occurrence_count DESC);
COMMIT;
"#,
    )
    .map_err(|e| CodeGraphError::Storage(e.to_string()))
}

pub(crate) fn collect_rows<T, F>(rows: rusqlite::MappedRows<'_, F>) -> Result<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
    }
    Ok(out)
}

pub(crate) fn tool_build_cluster_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ToolBuildCluster> {
    let occurrences_json: String = row.get(12)?;
    let occurrences =
        serde_json::from_str(&occurrences_json).map_err(|error| sqlite_json_error(12, error))?;
    let category_label: String = row.get(13)?;
    let member_ids_json: String = row.get(14)?;
    let member_cluster_ids: Vec<String> =
        serde_json::from_str(&member_ids_json).map_err(|error| sqlite_json_error(14, error))?;
    let reason: Option<String> = row.get(15)?;
    let ignored_by: Option<String> = row.get(16)?;
    let ignored_at: Option<String> = row.get(17)?;
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
        category: crate::tool_build::ToolBuildCategory::from_label(&category_label),
        member_cluster_ids,
        occurrences,
        ignored,
    })
}

pub(crate) fn sqlite_json_error(column: usize, error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
}

pub(crate) fn epoch_millis() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
