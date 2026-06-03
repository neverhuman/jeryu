//! Self-contained SQLite storage for the code graph.
//!
//! Mirrors the `SqliteStore` pattern in `jeryu-core::engine::storage`
//! (open -> apply schema via `execute_batch` -> atomic snapshot persist via a
//! transaction) but is fully self-contained: this crate opens its own
//! `codegraph.sqlite` and applies its own embedded schema. It never touches the
//! shared `db/migrations/` set and never edits `jeryu-core`.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateDepRow {
    /// Dependent crate.
    pub crate_name: String,
    /// Crate it depends on.
    pub depends_on: String,
}

/// A persistable snapshot of the code graph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphSnapshot {
    /// All indexed symbol rows.
    pub symbols: Vec<SymbolRow>,
    /// All recorded crate dependency edges.
    pub crate_deps: Vec<CrateDepRow>,
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
        tx.execute_batch("DELETE FROM codegraph_symbols; DELETE FROM codegraph_crate_deps;")
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

        Ok(snapshot)
    }

    /// Returns the on-disk path of this store.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}
