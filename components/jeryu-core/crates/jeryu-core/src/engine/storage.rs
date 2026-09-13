use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{Connection, DatabaseName, OpenFlags, TransactionBehavior, params};

use super::State;
use super::audit::AuditEntry;
use super::writer::WriterLease;
use crate::errors::{ForgeError, Result};

mod bound_reviews;
mod codec;
mod commissioning_effects;
mod load;
mod migrations;
mod persist;
mod publisher_enrollment;
mod ref_operations;
mod required_attempts;
mod snapshot;

#[cfg(test)]
mod tests;

use self::load::{backfill_missing_counters, load_state};
use self::persist::stage_state;

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
        let recovery = commissioning_effects::startup_barrier(&conn)?;
        if !recovery {
            apply_migrations(&conn)?;
        }
        let mut state = load_state(&conn)?;
        let backfilled = if recovery {
            0
        } else {
            backfill_missing_counters(&mut state)
                + super::backfill_default_branch_protections(&mut state)
        };
        drop(conn);
        if backfilled > 0 {
            store.persist(&state)?;
        }
        Ok((store, state))
    }

    pub(super) fn persist(&self, state: &State) -> Result<()> {
        let mut conn = self.connect()?;
        conn.execute_batch("PRAGMA temp_store = MEMORY;")
            .map_err(storage_error)?;
        let tx = conn.transaction().map_err(storage_error)?;
        persist_snapshot(&tx, state)?;
        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    /// Append one audit receipt through a fresh connection.
    ///
    /// `forge_audit_log` is independently owned and outside the State snapshot.
    /// Its append-only trail uses this dedicated path; ordinary State saves
    /// never delete or rewrite audit receipts.
    pub(super) fn append_audit(&self, entry: &AuditEntry) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        commissioning_effects::require_ordinary_admission(&tx)?;
        insert_audit(&tx, entry)?;
        tx.commit().map_err(storage_error)
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

    /// Barrier observation must not require a write when the guarded operation
    /// is a no-op. This connection cannot be used to persist an admitted effect.
    fn connect_observation(&self) -> Result<Connection> {
        self.writer.validate()?;
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(storage_error)?;
        self.writer.validate()?;
        Ok(conn)
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

    pub(super) fn required_publisher_custody(&self) -> Result<super::RequiredPublisherCustody> {
        self.writer.required_publisher_custody()
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn release_writer_lock_for_test(&self, index: usize) -> Result<()> {
        self.writer.release_lock_for_test(index)
    }
}

fn persist_snapshot(conn: &Connection, state: &State) -> Result<()> {
    commissioning_effects::require_ordinary_admission(conn)?;
    snapshot::create_tables(conn)?;
    stage_state(conn, state)?;
    snapshot::apply(conn)
}

fn insert_audit(conn: &Connection, entry: &AuditEntry) -> Result<()> {
    conn.execute(
        "INSERT INTO forge_audit_log (id, occurred_at, actor, action, subject, phase, detail_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.id,
            entry.occurred_at,
            entry.actor,
            entry.action,
            entry.subject,
            entry.phase,
            json(&entry.detail)?
        ],
    )
    .map_err(storage_error)?;
    Ok(())
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
