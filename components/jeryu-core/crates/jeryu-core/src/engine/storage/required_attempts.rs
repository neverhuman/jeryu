//! Private persistence for the publisher controller. No request transport may
//! invoke this module without Core enrollment, actor and origin admission.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use super::{SqliteStore, insert_audit, storage_error};
use crate::core::ref_operations::{canonical_json, sha256};
use crate::core::required_attempts::{
    DurableRequiredAttempt, MAX_SAFE_INTEGER, RequiredArtifactBytes, RequiredAttemptCompletion,
    RequiredAttemptConclusion, RequiredAttemptReservation, receive_artifacts,
};
use crate::{AuditEntry, ForgeError, Result};

impl SqliteStore {
    pub(in crate::core) fn reserve_required_attempt(
        &self,
        reservation: &RequiredAttemptReservation,
        now: DateTime<Utc>,
    ) -> Result<DurableRequiredAttempt> {
        reservation.validate()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let prior = tx
            .query_row(
                "SELECT id FROM forge_required_attempts WHERE repo_id = ?1 AND idempotency_key = ?2",
                params![reservation.binding.repository_id.to_string(), reservation.idempotency_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_error)?;
        if let Some(id) = prior {
            let existing = load_attempt(&tx, parse_id(&id)?)?;
            return if existing.reservation == *reservation {
                Ok(existing)
            } else {
                Err(conflict(
                    "attempt idempotency key has different immutable input",
                ))
            };
        }
        // Replay above retains fixed time. New reservations never extend an
        // existing admission and cannot be born expired or before issuance.
        if reservation.expires_at <= now {
            return Err(conflict("required-attempt reservation is expired"));
        }
        let previous: u64 = tx.query_row(
            "SELECT COALESCE(MAX(ordinal), 0) FROM forge_required_attempts WHERE repo_id = ?1 AND commit_sha = ?2 AND context = ?3",
            params![reservation.binding.repository_id.to_string(), reservation.binding.commit_sha, reservation.binding.context],
            |row| row.get(0),
        ).map_err(storage_error)?;
        let ordinal = previous
            .checked_add(1)
            .filter(|value| *value <= MAX_SAFE_INTEGER)
            .ok_or_else(|| conflict("required-attempt ordinal exhausted"))?;
        let reservation_json = canonical_json(reservation)?;
        let attempt = DurableRequiredAttempt {
            id: Uuid::new_v4(),
            ordinal,
            reservation: reservation.clone(),
            reservation_sha256: sha256(&reservation_json),
            reserved_at: now,
            audit_id: Uuid::new_v4(),
            completion: None,
        };
        tx.execute(
            "INSERT INTO forge_required_attempts (id, repo_id, commit_sha, context, ordinal, idempotency_key, reservation_json, attempt_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![attempt.id.to_string(), reservation.binding.repository_id.to_string(), reservation.binding.commit_sha,
                reservation.binding.context, ordinal, reservation.idempotency_key, reservation_json, canonical_json(&attempt)?],
        ).map_err(storage_error)?;
        insert_audit(&tx, &audit(&attempt, false))?;
        tx.commit().map_err(storage_error)?;
        Ok(attempt)
    }

    pub(in crate::core) fn complete_required_attempt(
        &self,
        id: Uuid,
        expected: &RequiredAttemptReservation,
        conclusion: RequiredAttemptConclusion,
        artifacts: &[RequiredArtifactBytes],
        now: DateTime<Utc>,
    ) -> Result<DurableRequiredAttempt> {
        expected.validate()?;
        let received = receive_artifacts(artifacts)?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut attempt = load_attempt(&tx, id)?;
        if attempt.reservation != *expected {
            return Err(conflict(
                "completion publisher/source/admission binding changed",
            ));
        }
        if let Some(completion) = &attempt.completion {
            return if completion.conclusion == conclusion && completion.artifacts == received {
                Ok(attempt)
            } else {
                Err(conflict(
                    "terminal required-attempt completion is immutable",
                ))
            };
        }
        if now < attempt.reserved_at || now >= attempt.reservation.expires_at {
            return Err(conflict(
                "required-attempt completion is outside its fixed window",
            ));
        }
        attempt.completion = Some(RequiredAttemptCompletion {
            conclusion,
            artifacts: received,
            completed_at: now,
            audit_id: Uuid::new_v4(),
            event_id: Uuid::new_v4(),
        });
        let completion = attempt.completion.as_ref().expect("completion assigned");
        for content in artifacts {
            let receipt = completion
                .artifacts
                .iter()
                .find(|row| row.name == content.name)
                .expect("receiving inventory matches validated input");
            tx.execute(
                "INSERT INTO forge_required_attempt_artifacts (attempt_id, name, sha256, size_bytes, content) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id.to_string(), receipt.name, receipt.sha256, receipt.size_bytes, content.bytes],
            ).map_err(storage_error)?;
        }
        let attempt_json = canonical_json(&attempt)?;
        if tx.execute(
            "UPDATE forge_required_attempts SET attempt_json = ?1, completed = 1 WHERE id = ?2 AND completed = 0",
            params![attempt_json, id.to_string()],
        ).map_err(storage_error)? != 1 {
            return Err(conflict("required attempt completed concurrently"));
        }
        insert_audit(&tx, &audit(&attempt, true))?;
        tx.execute(
            "INSERT INTO forge_required_attempt_outbox (id, attempt_id, repo_id, event_json) VALUES (?1, ?2, ?3, ?4)",
            params![completion.event_id.to_string(), id.to_string(), expected.binding.repository_id.to_string(), attempt_json],
        ).map_err(storage_error)?;
        tx.commit().map_err(storage_error)?;
        Ok(attempt)
    }

    pub(in crate::core) fn latest_required_attempt(
        &self,
        repository_id: Uuid,
        commit_sha: &str,
        context: &str,
    ) -> Result<Option<DurableRequiredAttempt>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(storage_error)?;
        let id = tx.query_row(
            "SELECT id FROM forge_required_attempts WHERE repo_id = ?1 AND commit_sha = ?2 AND context = ?3 ORDER BY ordinal DESC LIMIT 1",
            params![repository_id.to_string(), commit_sha, context],
            |row| row.get::<_, String>(0),
        ).optional().map_err(storage_error)?;
        id.map(|id| load_attempt(&tx, parse_id(&id)?)).transpose()
    }

    pub(in crate::core) fn get_required_attempt(&self, id: Uuid) -> Result<DurableRequiredAttempt> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(storage_error)?;
        load_attempt(&tx, id)
    }
}

fn load_attempt(conn: &Connection, id: Uuid) -> Result<DurableRequiredAttempt> {
    let row = conn.query_row(
        "SELECT repo_id, commit_sha, context, ordinal, idempotency_key, reservation_json, attempt_json, completed FROM forge_required_attempts WHERE id = ?1",
        params![id.to_string()],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
            row.get::<_, u64>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?,
            row.get::<_, String>(6)?, row.get::<_, bool>(7)?)),
    ).optional().map_err(storage_error)?.ok_or_else(|| ForgeError::NotFound(format!("required attempt {id}")))?;
    let attempt: DurableRequiredAttempt = serde_json::from_str(&row.6).map_err(storage_error)?;
    #[cfg(test)]
    after_record_read(id);
    attempt.reservation.validate().map_err(storage_error)?;
    let reservation = &attempt.reservation;
    let completion = attempt.completion.as_ref();
    if attempt.id != id
        || attempt.ordinal == 0
        || attempt.ordinal > MAX_SAFE_INTEGER
        || attempt.ordinal != row.3
        || attempt.audit_id.is_nil()
        || reservation.binding.repository_id.to_string() != row.0
        || reservation.binding.commit_sha != row.1
        || reservation.binding.context != row.2
        || reservation.idempotency_key != row.4
        || canonical_json(reservation)? != row.5
        || sha256(&row.5) != attempt.reservation_sha256
        || completion.is_some() != row.7
        || reservation.expires_at <= attempt.reserved_at
        || completion.is_some_and(|result| {
            result.audit_id.is_nil()
                || result.event_id.is_nil()
                || result.completed_at < attempt.reserved_at
                || result.completed_at >= reservation.expires_at
        })
    {
        return Err(ForgeError::Storage(
            "inconsistent required-attempt record".into(),
        ));
    }
    let mut statement = conn.prepare(
        "SELECT name, sha256, size_bytes, content FROM forge_required_attempt_artifacts WHERE attempt_id = ?1 ORDER BY name",
    ).map_err(storage_error)?;
    let contents = statement
        .query_map(params![id.to_string()], |row| {
            Ok((
                RequiredArtifactBytes {
                    name: row.get(0)?,
                    bytes: row.get(3)?,
                },
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
            ))
        })
        .map_err(storage_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    if let Some(completion) = completion {
        let input = contents
            .iter()
            .map(|(artifact, _, _)| RequiredArtifactBytes {
                name: artifact.name.clone(),
                bytes: artifact.bytes.clone(),
            })
            .collect::<Vec<_>>();
        let received = receive_artifacts(&input).map_err(storage_error)?;
        if received != completion.artifacts
            || received
                .iter()
                .zip(&contents)
                .any(|(receipt, (_, hash, size))| {
                    receipt.sha256 != *hash || receipt.size_bytes != *size
                })
        {
            return Err(ForgeError::Storage(
                "required artifact receiving bytes changed".into(),
            ));
        }
        let outbox = conn.query_row(
            "SELECT id, repo_id, event_json FROM forge_required_attempt_outbox WHERE attempt_id = ?1",
            params![id.to_string()], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        ).map_err(storage_error)?;
        if outbox.0 != completion.event_id.to_string()
            || outbox.1 != reservation.binding.repository_id.to_string()
            || outbox.2 != canonical_json(&attempt)?
        {
            return Err(ForgeError::Storage(
                "inconsistent required-attempt outbox".into(),
            ));
        }
    } else if !contents.is_empty() {
        return Err(ForgeError::Storage(
            "pending required attempt has terminal artifacts".into(),
        ));
    }
    Ok(attempt)
}

fn audit(attempt: &DurableRequiredAttempt, terminal: bool) -> AuditEntry {
    let completion = attempt.completion.as_ref().filter(|_| terminal);
    AuditEntry {
        id: completion
            .map_or(attempt.audit_id, |result| result.audit_id)
            .to_string(),
        occurred_at: completion
            .map_or(attempt.reserved_at, |result| result.completed_at)
            .to_rfc3339(),
        actor: attempt.reservation.binding.actor.login.clone(),
        action: "required_attempt".into(),
        subject: attempt.reservation.binding.repository_id.to_string(),
        phase: if terminal { "completed" } else { "requested" }.into(),
        detail: serde_json::json!({"attempt": attempt}),
    }
}

fn parse_id(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(storage_error)
}

fn conflict(message: &str) -> ForgeError {
    ForgeError::Conflict(message.into())
}

#[cfg(test)]
type ReadPause = (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>);

#[cfg(test)]
static READ_PAUSES: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<Uuid, ReadPause>>,
> = std::sync::LazyLock::new(std::sync::Mutex::default);

#[cfg(test)]
impl SqliteStore {
    pub(in crate::core) fn pause_required_attempt_read(&self, id: Uuid, pause: ReadPause) {
        assert!(READ_PAUSES.lock().unwrap().insert(id, pause).is_none());
    }
}

#[cfg(test)]
fn after_record_read(id: Uuid) {
    let pause = READ_PAUSES.lock().unwrap().remove(&id);
    if let Some((entered, release)) = pause {
        entered.send(()).unwrap();
        release
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
    }
}
