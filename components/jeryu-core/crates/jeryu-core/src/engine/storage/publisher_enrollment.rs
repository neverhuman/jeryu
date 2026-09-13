//! NEXT-SLICE PROPOSAL: no public enrollment or revocation transport.
//! The verified commissioning controller must retain the authority guard for
//! these operations, including detached signature and current actor checks.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use super::{SqliteStore, insert_audit, storage_error};
use crate::core::publisher_enrollment::{
    DurableRequiredPublisher, RequiredPublisherEnrollment, RequiredPublisherRevocation, actor_shape,
};
use crate::core::ref_operations::{canonical_json, sha256};
use crate::{AuditEntry, ForgeError, Result};

impl SqliteStore {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "private installer awaits verified commissioning authority"
        )
    )]
    pub(in crate::core) fn install_required_publisher(
        &self,
        enrollment: &RequiredPublisherEnrollment,
        expected_previous_sha256: Option<&str>,
        operation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<DurableRequiredPublisher> {
        enrollment.validate()?;
        if operation_id.is_nil() {
            return Err(ForgeError::Validation(
                "publisher installation operation is required".into(),
            ));
        }
        let digest = sha256(&canonical_json(enrollment)?);
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let replay: Option<(String, u64)> = tx.query_row(
            "SELECT publisher_id, revision FROM forge_required_publishers WHERE installation_operation_id = ?1",
            [operation_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(storage_error)?;
        if let Some((publisher_id, revision)) = replay {
            let record = load(
                &tx,
                Uuid::parse_str(&publisher_id).map_err(storage_error)?,
                revision,
            )?;
            if record.enrollment != *enrollment
                || record.enrollment_sha256 != digest
                || record.expected_previous_sha256.as_deref() != expected_previous_sha256
            {
                return Err(conflict(
                    "publisher operation id is bound to different enrollment",
                ));
            }
            return Ok(record);
        }
        if now < enrollment.issued_at || now >= enrollment.expires_at {
            return Err(conflict(
                "publisher installation is outside its fixed window",
            ));
        }
        let prior = latest(&tx, enrollment.publisher_id)?;
        match (prior.as_ref(), expected_previous_sha256) {
            (None, None) if enrollment.revision == 1 => {}
            (Some(prior), Some(expected))
                if prior.enrollment_sha256 == expected
                    && prior.enrollment.revision.checked_add(1) == Some(enrollment.revision) => {}
            _ => {
                return Err(conflict(
                    "publisher installation expected state or revision changed",
                ));
            }
        }
        let record = DurableRequiredPublisher {
            enrollment: enrollment.clone(),
            enrollment_sha256: digest,
            installation_operation_id: operation_id,
            installed_at: now,
            expected_previous_sha256: expected_previous_sha256.map(str::to_owned),
            audit_id: Uuid::new_v4(),
            revocation: None,
        };
        tx.execute(
            "INSERT INTO forge_required_publishers (publisher_id, revision, enrollment_sha256, installation_operation_id, record_json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![enrollment.publisher_id.to_string(), enrollment.revision, record.enrollment_sha256, operation_id.to_string(), canonical_json(&record)?],
        ).map_err(storage_error)?;
        insert_audit(
            &tx,
            &AuditEntry {
                id: record.audit_id.to_string(),
                occurred_at: now.to_rfc3339(),
                actor: enrollment.issuer.login.clone(),
                action: "required_publisher_enrollment".into(),
                subject: enrollment.publisher_id.to_string(),
                phase: "completed".into(),
                detail: serde_json::json!({"publisher": record, "expected_previous_sha256": expected_previous_sha256}),
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(record)
    }

    pub(in crate::core) fn latest_required_publisher(
        &self,
        publisher_id: Uuid,
    ) -> Result<Option<DurableRequiredPublisher>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(storage_error)?;
        latest(&tx, publisher_id)
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "private revocation writer awaits verified commissioning authority"
        )
    )]
    pub(in crate::core) fn revoke_required_publisher(
        &self,
        publisher_id: Uuid,
        expected_sha256: &str,
        revocation: &RequiredPublisherRevocation,
    ) -> Result<DurableRequiredPublisher> {
        actor_shape(&revocation.actor)?;
        if revocation.operation_id.is_nil()
            || revocation.reason.trim().is_empty()
            || revocation.reason.len() > 4096
            || revocation.reason.chars().any(char::is_control)
        {
            return Err(ForgeError::Validation(
                "publisher revocation identity and reason are required".into(),
            ));
        }
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut record = latest(&tx, publisher_id)?
            .ok_or_else(|| ForgeError::NotFound("publisher enrollment".into()))?;
        if record.enrollment_sha256 != expected_sha256 {
            return Err(conflict("publisher revocation expected enrollment changed"));
        }
        if let Some(previous) = &record.revocation {
            return if previous == revocation {
                Ok(record)
            } else {
                Err(conflict("publisher revocation is immutable"))
            };
        }
        if revocation.revoked_at < record.installed_at {
            return Err(conflict("publisher revocation precedes installation"));
        }
        record.revocation = Some(revocation.clone());
        let changed = tx.execute(
            "UPDATE forge_required_publishers SET revoked = 1, record_json = ?1 WHERE publisher_id = ?2 AND revision = ?3 AND revoked = 0",
            params![canonical_json(&record)?, publisher_id.to_string(), record.enrollment.revision],
        ).map_err(storage_error)?;
        if changed != 1 {
            return Err(conflict("publisher revocation raced"));
        }
        insert_audit(
            &tx,
            &AuditEntry {
                id: revocation.operation_id.to_string(),
                occurred_at: revocation.revoked_at.to_rfc3339(),
                actor: revocation.actor.login.clone(),
                action: "required_publisher_revocation".into(),
                subject: publisher_id.to_string(),
                phase: "completed".into(),
                detail: serde_json::json!({"publisher": record}),
            },
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(record)
    }
}

fn latest(conn: &Connection, publisher_id: Uuid) -> Result<Option<DurableRequiredPublisher>> {
    let revision: Option<u64> = conn.query_row(
        "SELECT revision FROM forge_required_publishers WHERE publisher_id = ?1 ORDER BY revision DESC LIMIT 1",
        [publisher_id.to_string()], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    revision
        .map(|revision| load(conn, publisher_id, revision))
        .transpose()
}

fn load(conn: &Connection, publisher_id: Uuid, revision: u64) -> Result<DurableRequiredPublisher> {
    let (digest, operation, json, revoked): (String, String, String, bool) = conn.query_row(
        "SELECT enrollment_sha256, installation_operation_id, record_json, revoked FROM forge_required_publishers WHERE publisher_id = ?1 AND revision = ?2",
        params![publisher_id.to_string(), revision], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).map_err(storage_error)?;
    let record: DurableRequiredPublisher = serde_json::from_str(&json).map_err(storage_error)?;
    record.enrollment.validate().map_err(storage_error)?;
    if let Some(revocation) = &record.revocation {
        actor_shape(&revocation.actor).map_err(storage_error)?;
        if revocation.operation_id.is_nil()
            || revocation.reason.trim().is_empty()
            || revocation.reason.len() > 4096
            || revocation.reason.chars().any(char::is_control)
            || revocation.revoked_at < record.installed_at
        {
            return Err(ForgeError::Storage(
                "malformed durable publisher revocation".into(),
            ));
        }
    }
    if record.enrollment.publisher_id != publisher_id
        || record.enrollment.revision != revision
        || record.enrollment_sha256 != digest
        || digest != sha256(&canonical_json(&record.enrollment)?)
        || record.installation_operation_id.to_string() != operation
        || record.installation_operation_id.is_nil()
        || record.audit_id.is_nil()
        || record.revocation.is_some() != revoked
        || record.installed_at < record.enrollment.issued_at
        || record.installed_at >= record.enrollment.expires_at
    {
        return Err(ForgeError::Storage(
            "publisher indexed identity disagrees with durable record".into(),
        ));
    }
    Ok(record)
}

fn conflict(message: &str) -> ForgeError {
    ForgeError::Conflict(message.into())
}
