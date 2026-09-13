//! Independent journal transactions, including composed State publication.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde_json::json;
use uuid::Uuid;

use super::super::ref_operations::*;
use super::super::{AuditEntry, State};
use super::{SqliteStore, insert_audit, parse_json, persist_snapshot, storage_error};
use crate::{ForgeError, Result};

const COLUMNS: &str = "id, repo_id, idempotency_key, intent_json, intent_sha256,
    qualification_sha256, marker_ref, marker_oid, prepared_at, prepared_audit_id, state, outcome_json";

struct OperationRow {
    id: String,
    repository_id: String,
    key: String,
    intent: String,
    intent_sha256: String,
    qualification_sha256: String,
    marker_ref: String,
    marker_oid: String,
    prepared_at: String,
    prepared_audit_id: String,
    state: String,
    outcome: Option<String>,
}

impl SqliteStore {
    pub(in crate::core) fn merge_operation_readback(
        &self,
        repository_id: Uuid,
        operation_id: Uuid,
    ) -> Result<MergeOperationReadback> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(storage_error)?;
        let operation = load_operation(&tx, repository_id, operation_id)?;
        if !matches!(
            operation.intent.operation,
            DurableRefOperationKind::Merge { .. }
        ) {
            return Err(ForgeError::Validation(
                "operation is not a merge operation".into(),
            ));
        }
        let delivery = operation
            .outcome
            .as_ref()
            .and_then(|outcome| outcome.event_id)
            .map(|event_id| load_event(&tx, repository_id, event_id))
            .transpose()?;
        if delivery.as_ref().is_some_and(|event| {
            event.repository_id != repository_id
                || event.operation_id != operation.id
                || event.operation != operation
        }) {
            return Err(ForgeError::Storage(
                "merge readback delivery belongs to a different operation".into(),
            ));
        }
        let readback = MergeOperationReadback {
            operation,
            delivery,
            observed_at: Utc::now(),
        };
        tx.commit().map_err(storage_error)?;
        Ok(readback)
    }

    pub(in crate::core) fn get_ref_operation(
        &self,
        repository_id: Uuid,
        operation_id: Uuid,
    ) -> Result<DurableRefOperation> {
        load_operation(&self.connect()?, repository_id, operation_id)
    }

    pub(in crate::core) fn get_ref_operation_by_key(
        &self,
        repository_id: Uuid,
        key: &str,
    ) -> Result<Option<DurableRefOperation>> {
        load_by_key(&self.connect()?, repository_id, key)
    }

    pub(in crate::core) fn prepare_ref_operation(
        &self,
        intent: &RefOperationIntent,
        now: DateTime<Utc>,
    ) -> Result<DurableRefOperation> {
        intent.validate()?;
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        if let Some(existing) = load_by_key(&tx, intent.repository_id, &intent.idempotency_key)? {
            return if existing.intent == *intent {
                Ok(existing)
            } else {
                Err(ForgeError::Conflict(
                    "ref operation idempotency key has different immutable input".into(),
                ))
            };
        }
        let unresolved: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM forge_ref_operations WHERE repo_id = ?1
             AND state IN ('prepared', 'reconciliation_required'))",
                [intent.repository_id.to_string()],
                |row| row.get(0),
            )
            .map_err(storage_error)?;
        if unresolved {
            return Err(ForgeError::WriterUnavailable(
                "repository has an unresolved ref operation".into(),
            ));
        }
        if intent.expires_at <= now {
            return Err(ForgeError::Validation(
                "new ref operation intent has expired".into(),
            ));
        }
        let id = Uuid::new_v4();
        let intent_json = canonical_json(intent)?;
        let operation = DurableRefOperation {
            id,
            intent: intent.clone(),
            intent_sha256: sha256(&intent_json),
            qualification_sha256: sha256(&canonical_json(&intent.qualification_snapshot)?),
            marker_ref: format!("refs/jeryu/operations/{id}"),
            marker_oid: marker_oid(intent)?,
            prepared_at: now,
            prepared_audit_id: Uuid::new_v4(),
            state: DurableRefOperationState::Prepared,
            outcome: None,
        };
        tx.execute(
            "INSERT INTO forge_ref_operations (id, repo_id, idempotency_key, intent_json,
             intent_sha256, qualification_sha256, marker_ref, marker_oid, prepared_at,
             prepared_audit_id, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'prepared')",
            params![operation.id.to_string(), intent.repository_id.to_string(), intent.idempotency_key,
                intent_json, operation.intent_sha256, operation.qualification_sha256,
                operation.marker_ref, operation.marker_oid, operation.prepared_at.to_rfc3339(),
                operation.prepared_audit_id.to_string()],
        ).map_err(storage_error)?;
        insert_operation_audit(
            &tx,
            &operation,
            operation.prepared_audit_id,
            "requested",
            operation.prepared_at,
        )?;
        tx.commit().map_err(storage_error)?;
        Ok(operation)
    }

    /// The caller holds authority/repository and State guards. Construct and
    /// persist the new snapshot in this same transaction, returning it for
    /// publication only after commit. No caller-owned State is mutated here.
    pub(in crate::core) fn reconcile_ref_operation(
        &self,
        repository_id: Uuid,
        operation_id: Uuid,
        observation: &RefOperationObservation,
        current_state: &State,
        on_committed: impl FnOnce(&mut State) -> Result<()>,
    ) -> Result<(DurableRefOperation, Option<State>)> {
        let mut conn = self.connect()?;
        conn.execute_batch("PRAGMA temp_store = MEMORY;")
            .map_err(storage_error)?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut operation = load_operation(&tx, repository_id, operation_id)?;
        if let Some(outcome) = &operation.outcome {
            return if outcome.observation == *observation {
                Ok((operation, None))
            } else {
                Err(ForgeError::Conflict("ref operation already has an immutable outcome; reconciliation requires separate authority".into()))
            };
        }
        let state = operation.classify(observation);
        let proposed = if state == DurableRefOperationState::Committed {
            let mut proposed = current_state.clone();
            on_committed(&mut proposed)?;
            persist_snapshot(&tx, &proposed)?;
            Some(proposed)
        } else {
            None
        };
        let recorded_at = Utc::now();
        let outcome = RefOperationOutcome {
            observation: observation.clone(),
            recorded_at,
            audit_id: Uuid::new_v4(),
            event_id: (state == DurableRefOperationState::Committed).then(Uuid::new_v4),
        };
        operation.state = state;
        operation.outcome = Some(outcome.clone());
        let updated = tx
            .execute(
                "UPDATE forge_ref_operations SET state = ?1, outcome_json = ?2
             WHERE id = ?3 AND repo_id = ?4 AND state = 'prepared'",
                params![
                    state_name(&operation.state),
                    canonical_json(&outcome)?,
                    operation_id.to_string(),
                    repository_id.to_string()
                ],
            )
            .map_err(storage_error)?;
        if updated != 1 {
            return Err(ForgeError::Conflict(
                "ref operation changed during outcome persistence".into(),
            ));
        }
        insert_operation_audit(
            &tx,
            &operation,
            outcome.audit_id,
            if operation.state == DurableRefOperationState::Committed {
                "completed"
            } else {
                "failed"
            },
            recorded_at,
        )?;
        if let Some(id) = outcome.event_id {
            let event = RefOperationEvent {
                id,
                operation_id,
                repository_id,
                created_at: recorded_at,
                operation: operation.clone(),
                delivered_at: None,
                delivery_receipt: None,
            };
            tx.execute(
                "INSERT INTO forge_ref_operation_outbox (id, operation_id, repo_id, created_at, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id.to_string(), operation_id.to_string(), repository_id.to_string(), recorded_at.to_rfc3339(), canonical_json(&event)?],
            ).map_err(storage_error)?;
        }
        tx.commit().map_err(storage_error)?;
        Ok((operation, proposed))
    }

    pub(in crate::core) fn pending_ref_operation_events(
        &self,
        repository_id: Uuid,
        limit: u32,
    ) -> Result<Vec<RefOperationEvent>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM forge_ref_operation_outbox WHERE repo_id = ?1 AND delivered_at IS NULL
             ORDER BY created_at, id LIMIT ?2",
        ).map_err(storage_error)?;
        let ids = stmt
            .query_map(params![repository_id.to_string(), limit], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage_error)?;
        ids.iter()
            .map(|id| load_event(&conn, repository_id, parse_uuid(id)?))
            .collect()
    }

    pub(in crate::core) fn acknowledge_ref_operation_event(
        &self,
        repository_id: Uuid,
        event_id: Uuid,
        receipt: &str,
    ) -> Result<RefOperationEvent> {
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage_error)?;
        let mut event = load_event(&tx, repository_id, event_id)?;
        if let Some(existing) = &event.delivery_receipt {
            return if existing == receipt {
                Ok(event)
            } else {
                Err(ForgeError::Conflict(
                    "outbox event already has a different delivery receipt".into(),
                ))
            };
        }
        let delivered_at = Utc::now();
        let updated = tx
            .execute(
                "UPDATE forge_ref_operation_outbox SET delivered_at = ?1, delivery_receipt = ?2
             WHERE id = ?3 AND repo_id = ?4 AND delivered_at IS NULL",
                params![
                    delivered_at.to_rfc3339(),
                    receipt,
                    event_id.to_string(),
                    repository_id.to_string()
                ],
            )
            .map_err(storage_error)?;
        if updated != 1 {
            return Err(ForgeError::Conflict("outbox delivery changed".into()));
        }
        tx.commit().map_err(storage_error)?;
        event.delivered_at = Some(delivered_at);
        event.delivery_receipt = Some(receipt.into());
        Ok(event)
    }
}

fn operation_row(row: &Row<'_>) -> rusqlite::Result<OperationRow> {
    Ok(OperationRow {
        id: row.get(0)?,
        repository_id: row.get(1)?,
        key: row.get(2)?,
        intent: row.get(3)?,
        intent_sha256: row.get(4)?,
        qualification_sha256: row.get(5)?,
        marker_ref: row.get(6)?,
        marker_oid: row.get(7)?,
        prepared_at: row.get(8)?,
        prepared_audit_id: row.get(9)?,
        state: row.get(10)?,
        outcome: row.get(11)?,
    })
}

fn load_by_key(
    conn: &Connection,
    repository_id: Uuid,
    key: &str,
) -> Result<Option<DurableRefOperation>> {
    conn.query_row(
        &format!(
            "SELECT {COLUMNS} FROM forge_ref_operations WHERE repo_id = ?1 AND idempotency_key = ?2"
        ),
        params![repository_id.to_string(), key],
        operation_row,
    )
    .optional()
    .map_err(storage_error)?
    .map(decode_operation)
    .transpose()
}

fn load_operation(
    conn: &Connection,
    repository_id: Uuid,
    operation_id: Uuid,
) -> Result<DurableRefOperation> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM forge_ref_operations WHERE repo_id = ?1 AND id = ?2"),
        params![repository_id.to_string(), operation_id.to_string()],
        operation_row,
    )
    .optional()
    .map_err(storage_error)?
    .map(decode_operation)
    .transpose()?
    .ok_or_else(|| {
        ForgeError::NotFound(format!(
            "ref operation {operation_id} in repository {repository_id}"
        ))
    })
}

fn decode_operation(row: OperationRow) -> Result<DurableRefOperation> {
    let intent: RefOperationIntent = parse_json(row.intent)?;
    intent
        .validate()
        .map_err(|error| ForgeError::Storage(format!("invalid stored ref intent: {error}")))?;
    let state = match row.state.as_str() {
        "prepared" => DurableRefOperationState::Prepared,
        "committed" => DurableRefOperationState::Committed,
        "aborted_not_applied" => DurableRefOperationState::AbortedNotApplied,
        "reconciliation_required" => DurableRefOperationState::ReconciliationRequired,
        _ => {
            return Err(ForgeError::Storage(
                "unknown stored ref operation state".into(),
            ));
        }
    };
    let operation = DurableRefOperation {
        id: parse_uuid(&row.id)?,
        intent,
        intent_sha256: row.intent_sha256,
        qualification_sha256: row.qualification_sha256,
        marker_ref: row.marker_ref,
        marker_oid: row.marker_oid,
        prepared_at: parse_time(&row.prepared_at)?,
        prepared_audit_id: parse_uuid(&row.prepared_audit_id)?,
        state,
        outcome: row.outcome.map(parse_json).transpose()?,
    };
    if operation.intent.repository_id.to_string() != row.repository_id
        || operation.intent.idempotency_key != row.key
        || operation.intent_sha256 != sha256(&canonical_json(&operation.intent)?)
        || operation.qualification_sha256
            != sha256(&canonical_json(&operation.intent.qualification_snapshot)?)
        || operation.marker_ref != format!("refs/jeryu/operations/{}", operation.id)
        || operation.marker_oid != marker_oid(&operation.intent)?
        || operation.outcome.is_none() != (operation.state == DurableRefOperationState::Prepared)
        || operation.outcome.as_ref().is_some_and(|outcome| {
            operation.classify(&outcome.observation) != operation.state
                || outcome.event_id.is_some()
                    != (operation.state == DurableRefOperationState::Committed)
        })
    {
        return Err(ForgeError::Storage(
            "stored ref operation binding does not match its immutable evidence".into(),
        ));
    }
    Ok(operation)
}

fn marker_oid(intent: &RefOperationIntent) -> Result<String> {
    intent
        .changes
        .iter()
        .map(|change| &change.result)
        .chain(intent.changes.iter().map(|change| &change.expected))
        .find_map(|value| {
            if let RefValue::Exact(oid) = value {
                Some(oid.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| ForgeError::Validation("operation marker needs a bound Git object".into()))
}

fn state_name(state: &DurableRefOperationState) -> &'static str {
    match state {
        DurableRefOperationState::Prepared => "prepared",
        DurableRefOperationState::Committed => "committed",
        DurableRefOperationState::AbortedNotApplied => "aborted_not_applied",
        DurableRefOperationState::ReconciliationRequired => "reconciliation_required",
    }
}

fn insert_operation_audit(
    conn: &Connection,
    operation: &DurableRefOperation,
    id: Uuid,
    phase: &str,
    occurred_at: DateTime<Utc>,
) -> Result<()> {
    insert_audit(
        conn,
        &AuditEntry {
            id: id.to_string(),
            occurred_at: occurred_at.to_rfc3339(),
            actor: operation.intent.actor.clone(),
            action: "repository.ref_operation".into(),
            subject: format!("repository:{}", operation.intent.repository_id),
            phase: phase.into(),
            detail: json!({"operation": operation}),
        },
    )
}

fn load_event(conn: &Connection, repository_id: Uuid, event_id: Uuid) -> Result<RefOperationEvent> {
    let row = conn
        .query_row(
            "SELECT operation_id, created_at, payload_json, delivered_at, delivery_receipt
         FROM forge_ref_operation_outbox WHERE repo_id = ?1 AND id = ?2",
            params![repository_id.to_string(), event_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(storage_error)?
        .ok_or_else(|| ForgeError::NotFound(format!("ref operation event {event_id}")))?;
    let mut event: RefOperationEvent = parse_json(row.2)?;
    let operation = load_operation(conn, repository_id, parse_uuid(&row.0)?)?;
    if event.id != event_id
        || event.repository_id != repository_id
        || event.operation_id != operation.id
        || event.operation != operation
        || event.created_at != parse_time(&row.1)?
        || event.delivered_at.is_some()
        || event.delivery_receipt.is_some()
        || operation.state != DurableRefOperationState::Committed
        || operation
            .outcome
            .as_ref()
            .and_then(|outcome| outcome.event_id)
            != Some(event_id)
        || row.3.is_some() != row.4.is_some()
    {
        return Err(ForgeError::Storage(
            "outbox event does not match its committed operation".into(),
        ));
    }
    event.delivered_at = row.3.as_deref().map(parse_time).transpose()?;
    event.delivery_receipt = row.4;
    Ok(event)
}

fn parse_uuid(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| ForgeError::Storage(error.to_string()))
}

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| ForgeError::Storage(error.to_string()))
}
