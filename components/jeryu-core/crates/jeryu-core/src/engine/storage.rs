use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{Connection, DatabaseName, OpenFlags, params};

use super::State;
use super::audit::AuditEntry;
use super::writer::WriterLease;
use crate::errors::{ForgeError, Result};

mod codec;
mod load;
mod migrations;
mod persist;

use self::load::{backfill_missing_counters, load_state};
use self::persist::{delete_all, persist_state};

use self::codec::*;
use self::migrations::apply_migrations;

#[derive(Debug, Clone)]
pub(super) struct SqliteStore {
    path: PathBuf,
    writer: Arc<WriterLease>,
}

impl SqliteStore {
    pub(super) fn open(path: impl AsRef<Path>, writer: Arc<WriterLease>) -> Result<(Self, State)> {
        let path = path.as_ref().to_path_buf();
        let store = Self { path, writer };
        let conn = store.connect()?;
        apply_migrations(&conn)?;
        let mut state = load_state(&conn)?;
        let mut backfilled = backfill_missing_counters(&mut state);
        backfilled += super::backfill_default_branch_protections(&mut state);
        drop(conn);
        if backfilled > 0 {
            store.persist(&state)?;
        }
        Ok((store, state))
    }

    pub(super) fn persist(&self, state: &State) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(storage_error)?;
        delete_all(&tx)?;
        persist_state(&tx, state)?;
        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    /// Append one audit receipt through a fresh connection.
    ///
    /// `forge_audit_log` is intentionally NOT part of `persist`/`delete_all`:
    /// the full-state rewrite must never wipe the trail, so audit writes take
    /// this dedicated path instead of riding the state snapshot.
    pub(super) fn append_audit(&self, entry: &AuditEntry) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO forge_audit_log (
              id, occurred_at, actor, action, subject, phase, detail_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                entry.id,
                entry.occurred_at,
                entry.actor,
                entry.action,
                entry.subject,
                entry.phase,
                json(&entry.detail)?,
            ],
        )
        .map_err(storage_error)?;
        Ok(())
    }

    /// All audit entries for one subject, oldest first.
    pub(super) fn list_audit(&self, subject: &str) -> Result<Vec<AuditEntry>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, occurred_at, actor, action, subject, phase, detail_json
                FROM forge_audit_log
                WHERE subject = ?1
                ORDER BY occurred_at, rowid
                "#,
            )
            .map_err(storage_error)?;
        let mut rows = stmt.query(params![subject]).map_err(storage_error)?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next().map_err(storage_error)? {
            entries.push(AuditEntry {
                id: row.get(0).map_err(storage_error)?,
                occurred_at: row.get(1).map_err(storage_error)?,
                actor: row.get(2).map_err(storage_error)?,
                action: row.get(3).map_err(storage_error)?,
                subject: row.get(4).map_err(storage_error)?,
                phase: row.get(5).map_err(storage_error)?,
                detail: parse_json(row.get(6).map_err(storage_error)?)?,
            });
        }
        Ok(entries)
    }

    fn connect(&self) -> Result<Connection> {
        self.writer.validate()?;
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(storage_error)?;
        self.writer.validate()?;
        if conn
            .is_readonly(DatabaseName::Main)
            .map_err(storage_error)?
        {
            return Err(ForgeError::WriterUnavailable(
                "the admitted database connection is read-only".to_string(),
            ));
        }
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(storage_error)?;
        Ok(conn)
    }

    pub(super) fn validate_writer(&self) -> Result<()> {
        self.writer.validate()
    }
}

pub(super) fn repo_id(
    repo_ids: &HashMap<(String, String), String>,
    owner: &str,
    repo: &str,
) -> Result<String> {
    repo_ids
        .get(&(owner.to_string(), repo.to_string()))
        .cloned()
        .ok_or_else(|| ForgeError::Storage(format!("missing repository row for {owner}/{repo}")))
}

pub(super) fn storage_error(error: impl std::fmt::Display) -> ForgeError {
    ForgeError::Storage(error.to_string())
}
