//! Append-only forge audit trail for privileged mutations.
//!
//! The `forge_audit_log` table is deliberately OUTSIDE the full-state-rewrite
//! persistence path: `SqliteStore::persist` deletes and reinserts every state
//! table on each mutation, so the audit table has no foreign key to
//! `repositories` and is never touched by that rewrite. Appends and reads go
//! through a fresh SQLite connection here instead.

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use super::ForgeCore;
use crate::errors::{ForgeError, Result};

/// Lifecycle phases an audit row may record, mirroring the SQL CHECK.
const AUDIT_PHASES: [&str; 3] = ["requested", "completed", "failed"];

/// One persisted audit receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// UUID-shaped row id (also returned by [`ForgeCore::append_audit`]).
    pub id: String,
    /// RFC 3339 timestamp the entry was appended at.
    pub occurred_at: String,
    /// Acting principal; the local control plane always writes `local`.
    pub actor: String,
    /// Dotted action name, e.g. `repository.delete`.
    pub action: String,
    /// Denormalized `owner/name` subject (no FK: it outlives the repo row).
    pub subject: String,
    /// One of `requested` / `completed` / `failed`.
    pub phase: String,
    /// Free-form JSON detail payload.
    pub detail: Value,
}

impl ForgeCore {
    /// Append one audit receipt and return its generated id.
    ///
    /// Writes through a fresh SQLite connection so the row is never part of
    /// the full-state rewrite. The in-memory core (`ForgeCore::new()`, no
    /// SQLite store) has nowhere durable to write: there the append is a
    /// validated no-op that still returns the generated id, so callers keep
    /// one code path in tests and production alike.
    pub fn append_audit(
        &self,
        action: &str,
        subject: &str,
        phase: &str,
        detail: Value,
    ) -> Result<String> {
        if action.trim().is_empty() {
            return Err(ForgeError::Validation(
                "audit action cannot be empty".to_string(),
            ));
        }
        if !AUDIT_PHASES.contains(&phase) {
            return Err(ForgeError::Validation(format!(
                "audit phase must be one of requested/completed/failed, got {phase:?}"
            )));
        }
        let entry = AuditEntry {
            id: Uuid::new_v4().to_string(),
            occurred_at: Utc::now().to_rfc3339(),
            actor: "local".to_string(),
            action: action.to_string(),
            subject: subject.to_string(),
            phase: phase.to_string(),
            detail,
        };
        let Some(storage) = &self.storage else {
            return Ok(entry.id);
        };
        storage.append_audit(&entry)?;
        Ok(entry.id)
    }

    /// All audit entries recorded for one subject, oldest first.
    ///
    /// The in-memory core has no store and therefore no trail: empty list.
    pub fn list_audit(&self, subject: &str) -> Result<Vec<AuditEntry>> {
        let Some(storage) = &self.storage else {
            return Ok(Vec::new());
        };
        storage.list_audit(subject)
    }
}
