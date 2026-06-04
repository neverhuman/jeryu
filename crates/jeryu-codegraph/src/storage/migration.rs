use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use crate::error::{CodeGraphError, Result};

use super::CodeGraphStore;
use super::schema::{
    DEFAULT_COMMIT_SHA, DEFAULT_REF_NAME, DEFAULT_ROOT, SCHEMA, SCHEMA_VERSION, now_rfc3339,
    schema_digest,
};
use super::types::{
    CacheStatus, CrateDepRow, GraphSnapshot, IndexReceipt, QueryOptions, RepoIdentity, SymbolRow,
};
use super::writes::{PersistRequest, insert_snapshot_rows};

impl CodeGraphStore {
    pub(crate) fn initialize(&self, conn: &mut Connection) -> Result<()> {
        let user_version = read_user_version(conn)?;
        if user_version == 0 && legacy_schema_detected(conn)? {
            self.migrate_legacy_database(conn)?;
        }
        self.apply_schema(conn)?;
        Ok(())
    }

    fn apply_schema(&self, conn: &mut Connection) -> Result<()> {
        let tx = conn
            .transaction()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        tx.execute_batch(SCHEMA)
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
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
        tx.execute(
            "INSERT OR IGNORE INTO codegraph_schema_migrations (version, name, applied_at, schema_digest) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                SCHEMA_VERSION,
                "v1 codegraph scoped storage",
                now_rfc3339(),
                schema_digest()
            ],
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        tx.execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION))
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        tx.commit()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(())
    }

    fn migrate_legacy_database(&self, conn: &mut Connection) -> Result<()> {
        let legacy_snapshot = self.load_legacy_snapshot(conn)?;
        let backup_path = self.write_legacy_backup()?;
        let tx = conn
            .transaction()
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        tx.execute_batch(
            r#"
            DROP TABLE IF EXISTS codegraph_outbox;
            DROP TABLE IF EXISTS codegraph_governance;
            DROP TABLE IF EXISTS codegraph_files;
            DROP TABLE IF EXISTS codegraph_symbols;
            DROP TABLE IF EXISTS codegraph_crate_deps;
            DROP TABLE IF EXISTS codegraph_index_runs;
            DROP TABLE IF EXISTS codegraph_schema_migrations;
            DROP TABLE IF EXISTS codegraph_repos;
            DROP TABLE IF EXISTS codegraph_meta;
            DROP TABLE IF EXISTS codegraph_slice_locks;
            "#,
        )
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        tx.execute_batch(SCHEMA)
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
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
        let repo = RepoIdentity::default();
        let scope = QueryOptions::default();
        let receipt = insert_snapshot_rows(
            tx,
            &legacy_snapshot,
            &PersistRequest {
                repo: &repo,
                ref_name: DEFAULT_REF_NAME,
                commit_sha: DEFAULT_COMMIT_SHA,
                root: DEFAULT_ROOT,
                query_options: &scope,
                cache_status: CacheStatus::Refreshed,
            },
        )?;
        write_migration_receipt(&self.path, &backup_path, &receipt)?;
        Ok(())
    }

    fn write_legacy_backup(&self) -> Result<PathBuf> {
        let mut backup = self.path.clone();
        let stamp = Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let stem = backup
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("codegraph.sqlite");
        backup.set_file_name(format!("{stem}.pre-schema-v1-{stamp}.bak"));
        if self.path.exists() {
            std::fs::copy(&self.path, &backup)
                .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        Ok(backup)
    }

    fn load_legacy_snapshot(&self, conn: &Connection) -> Result<GraphSnapshot> {
        let symbols = query_legacy_symbols(conn)?;
        let crate_deps = query_legacy_crate_deps(conn)?;
        Ok(GraphSnapshot {
            symbols,
            crate_deps,
        })
    }
}

fn read_user_version(conn: &Connection) -> Result<i64> {
    conn.query_row("PRAGMA user_version;", [], |row| row.get(0))
        .map_err(|e| CodeGraphError::Storage(e.to_string()))
}

fn legacy_schema_detected(conn: &Connection) -> Result<bool> {
    if !table_exists(conn, "codegraph_symbols")? || !table_exists(conn, "codegraph_crate_deps")? {
        return Ok(false);
    }
    let columns = table_columns(conn, "codegraph_symbols")?;
    Ok(columns.iter().any(|column| column == "crate_name")
        || (columns.iter().any(|column| column == "crate")
            && !columns.iter().any(|column| column == "repo_id")))
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [table],
            |_| Ok(()),
        )
        .optional()
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?
        .is_some();
    Ok(exists)
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| row.get(1))
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    let mut columns = Vec::new();
    for row in rows {
        columns.push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
    }
    Ok(columns)
}

fn query_legacy_symbols(conn: &Connection) -> Result<Vec<SymbolRow>> {
    if !table_exists(conn, "codegraph_symbols")? {
        return Ok(Vec::new());
    }
    let columns = table_columns(conn, "codegraph_symbols")?;
    if !columns.iter().any(|column| column == "crate_name")
        && !columns.iter().any(|column| column == "crate")
    {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT crate, file, symbol, kind, is_public, line FROM codegraph_symbols \
             ORDER BY crate, file, symbol",
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
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
    }
    Ok(out)
}

fn query_legacy_crate_deps(conn: &Connection) -> Result<Vec<CrateDepRow>> {
    if !table_exists(conn, "codegraph_crate_deps")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare("SELECT crate, depends_on FROM codegraph_crate_deps ORDER BY crate, depends_on")
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CrateDepRow {
                crate_name: row.get(0)?,
                depends_on: row.get(1)?,
            })
        })
        .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| CodeGraphError::Storage(e.to_string()))?);
    }
    Ok(out)
}

fn write_migration_receipt(
    db_path: &Path,
    backup_path: &Path,
    receipt: &IndexReceipt,
) -> Result<()> {
    let receipt_path = db_path.with_extension("migration.json");
    let payload = json!({
        "schema_version": SCHEMA_VERSION,
        "schema_digest": schema_digest(),
        "backup_path": backup_path.display().to_string(),
        "receipt": receipt,
    });
    let encoded =
        serde_json::to_vec_pretty(&payload).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    std::fs::write(&receipt_path, encoded).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
    Ok(())
}
