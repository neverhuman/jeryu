//! Self-contained SQLite storage for the code graph.
//!
//! The store owns its own SQLite database, applies versioned migrations on
//! open, and scopes persisted graph rows by repository and commit. A refresh
//! appends a new index receipt and outbox event; a cache hit reuses the latest
//! matching snapshot without mutating history.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{CodeGraphError, Result};

mod migration;
mod reads;
mod schema;
mod types;
mod writes;

pub use schema::{SCHEMA, SCHEMA_VERSION, default_db_path, schema_digest};
pub use types::{
    CacheStatus, CrateDepRow, FileRecord, GovernanceRecord, GraphSnapshot, GraphStats,
    IndexReceipt, QueryOptions, RepoIdentity, SymbolRow,
};

/// Self-contained SQLite store for the code graph.
#[derive(Debug, Clone)]
pub struct CodeGraphStore {
    path: PathBuf,
}

impl CodeGraphStore {
    /// Opens (creating if needed) the store at `path` and applies the schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        }
        let store = Self { path };
        let mut conn = store.connect()?;
        store.initialize(&mut conn)?;
        Ok(store)
    }

    /// Opens the store at the default `~/.jeryu/codegraph.sqlite` path.
    pub fn open_default() -> Result<Self> {
        Self::open(default_db_path())
    }

    pub(crate) fn connect(&self) -> Result<Connection> {
        let conn =
            Connection::open(&self.path).map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| CodeGraphError::Storage(e.to_string()))?;
        Ok(conn)
    }

    /// Returns the on-disk path of this store.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}
