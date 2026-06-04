//! Self-contained SQLite storage for the code graph.
//!
//! Mirrors the `SqliteStore` pattern in `jeryu-core::engine::storage`
//! (open -> apply schema via `execute_batch` -> atomic snapshot persist via a
//! transaction) but is fully self-contained: this crate opens its own
//! `codegraph.sqlite` and applies its own embedded schema. It never touches the
//! shared `db/migrations/` set and never edits `jeryu-core`.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::error::{CodeGraphError, Result};

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

CREATE TABLE IF NOT EXISTS codegraph_slice_locks (
    id           TEXT PRIMARY KEY,
    crate        TEXT NOT NULL,
    prefixes_json TEXT NOT NULL,
    locked_by    TEXT NOT NULL,
    reason       TEXT NOT NULL,
    locked_at    TEXT NOT NULL,
    expires_at   TEXT
);

CREATE TABLE IF NOT EXISTS codegraph_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO codegraph_meta (key, value) VALUES ('schema_version', '2');
"#;

/// Default database location under the user's `~/.jeryu/` directory.
#[must_use]
pub fn default_db_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".jeryu").join("codegraph.sqlite")
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

/// A persistable snapshot of the code graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    /// All indexed symbol rows.
    pub symbols: Vec<SymbolRow>,
    /// All recorded crate dependency edges.
    pub crate_deps: Vec<CrateDepRow>,
    /// All recorded symbol reference rows.
    pub symbol_refs: Vec<SymbolRefRow>,
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
             DELETE FROM codegraph_symbol_refs;",
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
