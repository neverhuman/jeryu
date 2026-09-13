//! Private durable operations. Every mutation is one IMMEDIATE transaction;
//! controller admission must already hold authority and repository custody.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use super::{SqliteStore, storage_error};
use crate::core::ReviewActorBinding;
use crate::core::commissioning_canonical::{canonical_payload, parse_contract};
use crate::core::commissioning_effects::*;
use crate::{ForgeError, Result};

impl SqliteStore {
    pub(in crate::core) fn reserve_commissioning_restore(
        &self,
        scope: &CommissioningRestoreScope,
        now: DateTime<Utc>,
    ) -> Result<CommissioningRestoreOperation> {
        scope.validate()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let prior = tx.query_row(
            "SELECT id FROM forge_commissioning_operations WHERE contract_id = ?1 AND idempotency_key = ?2",
            params![scope.contract_id.to_string(), scope.request.idempotency_key], |row| row.get::<_, String>(0),
        ).optional().map_err(storage_error)?;
        if let Some(id) = prior {
            let existing = load(&tx, parse_id(&id)?)?;
            return if existing.scope == *scope {
                load_revision(&tx, existing.id, Some(1))
            } else {
                Err(conflict(
                    "commissioning reservation replay changed immutable input",
                ))
            };
        }
        scope.require_window(now)?;
        if barrier(&tx, Some(scope.backing_pair_id))?.is_some() {
            return Err(conflict(
                "another restoration holds the operation-wide barrier",
            ));
        }
        let id = Uuid::new_v4();
        tx.execute("INSERT INTO forge_commissioning_operations (id, contract_id, repo_id, pair_id, idempotency_key) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id.to_string(), scope.contract_id.to_string(), scope.repository_id.to_string(), scope.backing_pair_id.to_string(), scope.request.idempotency_key]).map_err(storage_error)?;
        let record = CommissioningJournalRecord {
            operation_id: id,
            revision: 1,
            previous_sha256: "0".repeat(64),
            recorded_at: now,
            actor: scope.operator.clone(),
            body: CommissioningJournalEvent::Reserved {
                scope: Box::new(scope.clone()),
            },
        };
        let operation = append(&tx, None, &record)?;
        tx.commit().map_err(storage_error)?;
        Ok(operation)
    }

    pub(in crate::core) fn commissioning_operation(
        &self,
        id: Uuid,
    ) -> Result<CommissioningRestoreOperation> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(storage_error)?;
        load(&tx, id)
    }

    #[cfg(test)]
    pub(in crate::core) fn commissioning_barrier(
        &self,
        pair: Uuid,
    ) -> Result<Option<CommissioningRestoreOperation>> {
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(storage_error)?;
        barrier(&tx, Some(pair))
    }

    pub(in crate::core) fn require_ordinary_mutation(&self) -> Result<()> {
        let mut conn = self.connect_observation()?;
        let tx = conn.transaction().map_err(storage_error)?;
        require_ordinary_admission(&tx)
    }

    pub(in crate::core) fn admit_commissioning_step(
        &self,
        id: Uuid,
        request: &CommissioningStepRequest,
        actor: &ReviewActorBinding,
        now: DateTime<Utc>,
    ) -> Result<CommissioningRestoreOperation> {
        request.expected.validate()?;
        request.step.target_name()?;
        digest_shape(&request.local_custody_sha256)?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let prior = load(&tx, id)?;
        if let Some(step) = prior.steps.iter().find(|s| s.expected == request.expected) {
            return if step.step == request.step
                && step.local_custody_sha256 == request.local_custody_sha256
                && *actor == prior.scope.operator
            {
                load_revision(&tx, id, request.expected.revision.checked_add(1))
            } else {
                Err(conflict("step replay changed its immutable admission"))
            };
        }
        if prior.current != request.expected {
            return Err(conflict("commissioning journal revision changed"));
        }
        let step = CommissioningAdmittedStep {
            id: Uuid::new_v4(),
            expected: request.expected.clone(),
            step: request.step,
            local_custody_sha256: request.local_custody_sha256.clone(),
            admitted_at: now,
            completion: None,
        };
        let record = record(
            &prior,
            actor,
            now,
            CommissioningJournalEvent::StepAdmitted { step },
        )?;
        let result = append(&tx, Some(prior), &record)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }

    pub(in crate::core) fn complete_commissioning_step(
        &self,
        id: Uuid,
        request: &CommissioningCompletionRequest,
        actor: &ReviewActorBinding,
        recording: CommissioningRecordingAuthority,
        now: DateTime<Utc>,
    ) -> Result<CommissioningRestoreOperation> {
        request.expected.validate()?;
        evidence_shape(&request.evidence)?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let prior = load(&tx, id)?;
        let step = prior
            .steps
            .iter()
            .find(|step| step.id == request.step_id)
            .ok_or_else(|| conflict("completion does not target an admitted step"))?;
        if let Some(completion) = &step.completion {
            // A newly authorized recovery recorder may read an identical old
            // outcome. It does not change its original recorder or time.
            return if completion.expected == request.expected
                && completion.outcome == request.outcome
                && completion.evidence == request.evidence
            {
                load_revision(&tx, id, request.expected.revision.checked_add(1))
            } else {
                Err(conflict(
                    "immutable commissioning completion replay differs",
                ))
            };
        }
        if prior.current != request.expected {
            return Err(conflict("commissioning completion revision changed"));
        }
        let completion = CommissioningStepCompletion {
            expected: request.expected.clone(),
            outcome: request.outcome,
            evidence: request.evidence.clone(),
            evidence_sha256: bytes_hash(&request.evidence),
            recorder: actor.clone(),
            recording_authority: recording,
            recorded_at: now,
            authorizes_progress: recording == CommissioningRecordingAuthority::CurrentOperator
                && *actor == prior.scope.operator
                && request.outcome == CommissioningEffectOutcome::Verified
                && prior.scope.require_window(now).is_ok()
                && !prior.recovery_required,
        };
        let record = record(
            &prior,
            actor,
            now,
            CommissioningJournalEvent::StepCompleted {
                step_id: request.step_id,
                completion,
            },
        )?;
        let result = append(&tx, Some(prior), &record)?;
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }

    /// Normal in-window closure only. Explicit post-expiry recovery reconciliation
    /// needs a separately reviewed fresh authority path and is not supplied here.
    pub(in crate::core) fn close_commissioning_operation(
        &self,
        id: Uuid,
        expected: &CommissioningRevision,
        actor: &ReviewActorBinding,
        evidence: &[u8],
        now: DateTime<Utc>,
    ) -> Result<CommissioningRestoreOperation> {
        expected.validate()?;
        evidence_shape(evidence)?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let prior = load(&tx, id)?;
        if prior.closed_at.is_some() {
            let json: String = tx.query_row(
                "SELECT record_json FROM forge_commissioning_records WHERE operation_id = ?1 AND revision = ?2",
                params![id.to_string(), prior.current.revision], |row| row.get(0),
            ).map_err(storage_error)?;
            let final_record: CommissioningJournalRecord = parse_contract(json.as_bytes())?;
            return if final_record.actor == *actor
                && final_record.previous_sha256 == expected.record_sha256
                && expected.revision.checked_add(1) == Some(final_record.revision)
                && matches!(&final_record.body, CommissioningJournalEvent::Closed { final_evidence, .. } if final_evidence == evidence)
            {
                Ok(prior)
            } else {
                Err(conflict(
                    "immutable terminal commissioning operation differs",
                ))
            };
        }
        if &prior.current != expected {
            return Err(conflict("commissioning closure revision changed"));
        }
        let record = record(
            &prior,
            actor,
            now,
            CommissioningJournalEvent::Closed {
                final_evidence: evidence.to_vec(),
                final_evidence_sha256: bytes_hash(evidence),
            },
        )?;
        let result = append(&tx, Some(prior), &record)?;
        if tx
            .execute(
                "UPDATE forge_commissioning_operations SET closed = 1 WHERE id = ?1 AND closed = 0",
                params![id.to_string()],
            )
            .map_err(storage_error)?
            != 1
        {
            return Err(conflict(
                "commissioning barrier changed during verified closure",
            ));
        }
        tx.commit().map_err(storage_error)?;
        Ok(result)
    }
}

fn record(
    prior: &CommissioningRestoreOperation,
    actor: &ReviewActorBinding,
    now: DateTime<Utc>,
    body: CommissioningJournalEvent,
) -> Result<CommissioningJournalRecord> {
    let revision = prior
        .current
        .revision
        .checked_add(1)
        .filter(|value| *value <= 9_007_199_254_740_991)
        .ok_or_else(|| conflict("commissioning revision exhausted"))?;
    Ok(CommissioningJournalRecord {
        operation_id: prior.id,
        revision,
        previous_sha256: prior.current.record_sha256.clone(),
        recorded_at: now,
        actor: actor.clone(),
        body,
    })
}

fn append(
    conn: &Connection,
    prior: Option<CommissioningRestoreOperation>,
    record: &CommissioningJournalRecord,
) -> Result<CommissioningRestoreOperation> {
    let operation = apply_record(prior, record)?;
    let json = String::from_utf8(canonical_payload(record)?).map_err(storage_error)?;
    conn.execute("INSERT INTO forge_commissioning_records (operation_id, revision, record_sha256, record_json, terminal) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![record.operation_id.to_string(), record.revision, operation.current.record_sha256, json,
            matches!(record.body, CommissioningJournalEvent::Closed { .. })]).map_err(storage_error)?;
    Ok(operation)
}

fn load(conn: &Connection, id: Uuid) -> Result<CommissioningRestoreOperation> {
    load_revision(conn, id, None)
}

/// Validate the entire receiving chain and current header/barrier before
/// returning an original immutable mutation response at its recorded revision.
/// Ordinary readback always requests the current complete operation instead.
fn load_revision(
    conn: &Connection,
    id: Uuid,
    requested: Option<u64>,
) -> Result<CommissioningRestoreOperation> {
    let header = conn.query_row("SELECT contract_id, repo_id, pair_id, idempotency_key, closed FROM forge_commissioning_operations WHERE id = ?1",
        params![id.to_string()], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, bool>(4)?)))
        .optional().map_err(storage_error)?.ok_or_else(|| ForgeError::NotFound("commissioning operation".into()))?;
    let mut statement = conn.prepare("SELECT revision, record_sha256, record_json, terminal FROM forge_commissioning_records WHERE operation_id = ?1 ORDER BY revision").map_err(storage_error)?;
    let mut rows = statement
        .query(params![id.to_string()])
        .map_err(storage_error)?;
    let mut operation = None;
    let mut historical = None;
    let mut previous_time = None;
    let mut count = 0;
    while let Some(row) = rows.next().map_err(storage_error)? {
        count += 1;
        if count > 128 {
            return Err(storage_error(
                "commissioning record count exceeds fixed target plan",
            ));
        }
        let revision: u64 = row.get(0).map_err(storage_error)?;
        let digest: String = row.get(1).map_err(storage_error)?;
        let json: String = row.get(2).map_err(storage_error)?;
        let terminal: bool = row.get(3).map_err(storage_error)?;
        let record: CommissioningJournalRecord =
            parse_contract(json.as_bytes()).map_err(storage_error)?;
        if record.operation_id != id
            || revision != record.revision
            || previous_time.is_some_and(|time| record.recorded_at < time)
            || terminal != matches!(record.body, CommissioningJournalEvent::Closed { .. })
            || canonical_payload(&record)?.as_slice() != json.as_bytes()
        {
            return Err(storage_error(
                "commissioning durable record projection differs",
            ));
        }
        previous_time = Some(record.recorded_at);
        let next = apply_record(operation, &record).map_err(storage_error)?;
        if next.current.record_sha256 != digest {
            return Err(storage_error("commissioning durable record hash differs"));
        }
        if requested == Some(revision) {
            historical = Some(next.clone());
        }
        operation = Some(next);
    }
    let operation =
        operation.ok_or_else(|| storage_error("commissioning operation lacks reservation"))?;
    if operation.scope.contract_id.to_string() != header.0
        || operation.scope.repository_id.to_string() != header.1
        || operation.scope.backing_pair_id.to_string() != header.2
        || operation.scope.request.idempotency_key != header.3
        || operation.closed_at.is_some() != header.4
    {
        return Err(storage_error(
            "commissioning header or operation-wide barrier differs",
        ));
    }
    match requested {
        Some(_) => {
            historical.ok_or_else(|| storage_error("commissioning replay revision is absent"))
        }
        None => Ok(operation),
    }
}

/// Startup may encounter a predecessor schema. Once the commissioning schema
/// exists, reconstruct the retained journal before allowing migrations or
/// backfills; an interrupted operation must reopen without either effect.
pub(super) fn startup_barrier(conn: &Connection) -> Result<bool> {
    Ok(commissioning_schema_present(conn)? && barrier(conn, None)?.is_some())
}

fn commissioning_schema_present(conn: &Connection) -> Result<bool> {
    let (objects, tables): (u32, u32) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(type = 'table'), 0) FROM sqlite_schema WHERE name IN ('forge_commissioning_operations', 'forge_commissioning_records')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(storage_error)?;
    match (objects, tables) {
        (0, 0) => Ok(false),
        (2, 2) => Ok(true),
        _ => Err(ForgeError::WriterUnavailable(
            "partial or substituted commissioning schema requires recovery before migration".into(),
        )),
    }
}

/// The caller owns the transaction or coordinator custody across its effect.
/// Check all retained operations, including terminal-chain integrity. Neither a
/// forged pair ID nor a changed `closed` projection can open ordinary admission.
pub(super) fn require_ordinary_admission(conn: &Connection) -> Result<()> {
    match barrier(conn, None) {
        Ok(None) => Ok(()),
        Ok(Some(operation)) => Err(ForgeError::WriterUnavailable(format!(
            "commissioning operation {} holds ordinary writer admission until verified closure",
            operation.id
        ))),
        Err(error) => Err(ForgeError::WriterUnavailable(format!(
            "commissioning barrier requires recovery before ordinary mutation: {error}"
        ))),
    }
}

fn barrier(conn: &Connection, pair: Option<Uuid>) -> Result<Option<CommissioningRestoreOperation>> {
    if !commissioning_schema_present(conn)? {
        return Err(ForgeError::WriterUnavailable(
            "commissioning schema is absent from an initialized writer".into(),
        ));
    }
    let orphaned: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM forge_commissioning_records r LEFT JOIN forge_commissioning_operations o ON o.id = r.operation_id WHERE o.id IS NULL)",
            [],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if orphaned {
        return Err(ForgeError::WriterUnavailable(
            "orphaned commissioning records require recovery before writer admission".into(),
        ));
    }
    // The indexed `closed` field is a projection, not independent authority.
    // Even purportedly closed operations must reconstruct a valid terminal
    // record before absence of a barrier is established.
    let mut statement = conn
        .prepare("SELECT id FROM forge_commissioning_operations ORDER BY id")
        .map_err(storage_error)?;
    let mut rows = statement.query([]).map_err(storage_error)?;
    let mut active = None;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let id: String = row.get(0).map_err(storage_error)?;
        let operation = load(conn, parse_id(&id)?)?;
        if operation.blocks_admission() {
            if pair.is_some_and(|pair| operation.scope.backing_pair_id != pair) || active.is_some()
            {
                return Err(ForgeError::WriterUnavailable(
                    "active commissioning operations disagree with exclusive backing-pair custody"
                        .into(),
                ));
            }
            active = Some(operation);
        }
    }
    Ok(active)
}
fn parse_id(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(storage_error)
}
