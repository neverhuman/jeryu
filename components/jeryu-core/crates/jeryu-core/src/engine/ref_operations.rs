//! Durable recovery primitives for the forthcoming Core-owned Git operation.
//!
//! Mutation entrypoints are private to Core. They do not authenticate a login,
//! obtain Git observations, dispatch Git, or activate the existing merge routes.
//! The owning execute_merge/broker must acquire qualification and observations
//! independently under its continuously held guards before composing these APIs.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{AuthenticatedActor, ForgeCore, State, mutation::require_repository_admissible};
use crate::UserRole;
use crate::{ForgeError, Result};

#[cfg(test)]
mod tests;

/// Absence is an explicit expectation, never an omitted CAS precondition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "oid", rename_all = "snake_case")]
pub enum RefValue {
    Absent,
    Exact(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefChangeIntent {
    pub reference: String,
    pub expected: RefValue,
    pub result: RefValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DurableRefOperationKind {
    Merge {
        pull_request_id: Uuid,
        source_repository_id: Uuid,
        source_ref: String,
        source_head: String,
    },
    RefUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefOperationIntent {
    pub repository_id: Uuid,
    pub idempotency_key: String,
    pub actor: String,
    pub operation: DurableRefOperationKind,
    pub changes: Vec<RefChangeIntent>,
    /// Complete authoritative snapshot, retained rather than a caller's digest.
    /// Required keys: policy_revision, policy, review_ids, attempt_ids,
    /// actor_authorization, blockers. Additional evidence is preserved verbatim.
    pub qualification_snapshot: Value,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableRefOperationState {
    Prepared,
    Committed,
    AbortedNotApplied,
    ReconciliationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedRef {
    pub reference: String,
    pub value: RefValue,
}

/// Recorded backend evidence, not a trusted caller declaration of success.
/// The future backend must obtain it independently; no wire endpoint accepts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefOperationObservation {
    Observed {
        refs: Vec<ObservedRef>,
        marker: RefValue,
        /// Evidence beyond the marker prevents an unchanged ref being called
        /// not-applied when a transaction may nevertheless have occurred.
        additional_application_evidence: bool,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefOperationOutcome {
    pub observation: RefOperationObservation,
    pub recorded_at: DateTime<Utc>,
    pub audit_id: Uuid,
    /// Only committed operations produce an executable outbox event.
    pub event_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableRefOperation {
    pub id: Uuid,
    pub intent: RefOperationIntent,
    pub intent_sha256: String,
    pub qualification_sha256: String,
    pub marker_ref: String,
    pub marker_oid: String,
    pub prepared_at: DateTime<Utc>,
    pub prepared_audit_id: Uuid,
    pub state: DurableRefOperationState,
    pub outcome: Option<RefOperationOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefOperationEvent {
    pub id: Uuid,
    pub operation_id: Uuid,
    pub repository_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub operation: DurableRefOperation,
    pub delivered_at: Option<DateTime<Utc>>,
    pub delivery_receipt: Option<String>,
}

/// One durable read snapshot. The observation is the persisted backend evidence
/// in operation.outcome, not a fresh Git observation or permission to dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeOperationReadback {
    pub operation: DurableRefOperation,
    pub delivery: Option<RefOperationEvent>,
    pub observed_at: DateTime<Utc>,
}

impl RefOperationIntent {
    pub(super) fn validate(&self) -> Result<()> {
        if self.repository_id.is_nil()
            || self.idempotency_key.trim().is_empty()
            || self.idempotency_key.len() > 256
            || self.actor.trim().is_empty()
            || self.changes.is_empty()
            || self.changes.len() > 1024
        {
            return Err(invalid("incomplete ref operation identity or changes"));
        }
        let mut refs = BTreeSet::new();
        for change in &self.changes {
            validate_ref(&change.reference)?;
            validate_value(&change.expected)?;
            validate_value(&change.result)?;
            if !refs.insert(&change.reference) || change.expected == change.result {
                return Err(invalid("duplicate ref or non-changing ref operation"));
            }
        }
        if let DurableRefOperationKind::Merge {
            pull_request_id,
            source_repository_id,
            source_ref,
            source_head,
        } = &self.operation
        {
            if pull_request_id.is_nil() || source_repository_id.is_nil() {
                return Err(invalid(
                    "merge requires exact PR and source repository UUIDs",
                ));
            }
            validate_ref(source_ref)?;
            if !source_ref.starts_with("refs/heads/") {
                return Err(invalid("merge source must be a branch"));
            }
            validate_oid(source_head)?;
            if self.changes.len() != 1
                || !self.changes[0].reference.starts_with("refs/heads/")
                || !matches!(self.changes[0].expected, RefValue::Exact(_))
                || self.changes[0].result != RefValue::Exact(source_head.clone())
            {
                return Err(invalid(
                    "merge must update one existing branch to its exact source head",
                ));
            }
        }
        let snapshot = &self.qualification_snapshot;
        if snapshot
            .get("policy_revision")
            .and_then(Value::as_str)
            .is_none_or(|s| s.trim().is_empty())
            || !snapshot.get("policy").is_some_and(Value::is_object)
            || snapshot
                .get("actor_authorization")
                .and_then(Value::as_object)
                .is_none_or(|a| a.is_empty())
            || !snapshot
                .get("blockers")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
        {
            return Err(invalid(
                "complete unblocked qualification snapshot required",
            ));
        }
        for key in ["review_ids", "attempt_ids"] {
            let ids = snapshot
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("qualification review/attempt identities required"))?;
            let mut unique = BTreeSet::new();
            for id in ids {
                let id = id
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .filter(|id| !id.is_nil())
                    .ok_or_else(|| invalid("invalid qualification evidence UUID"))?;
                if !unique.insert(id) {
                    return Err(invalid("duplicate qualification evidence UUID"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn repositories(&self) -> Vec<Uuid> {
        let mut ids = vec![self.repository_id];
        if let DurableRefOperationKind::Merge {
            source_repository_id,
            ..
        } = self.operation
        {
            ids.push(source_repository_id);
        }
        ids
    }
}

impl DurableRefOperation {
    pub(super) fn classify(
        &self,
        observation: &RefOperationObservation,
    ) -> DurableRefOperationState {
        let RefOperationObservation::Observed {
            refs,
            marker,
            additional_application_evidence,
        } = observation
        else {
            return DurableRefOperationState::ReconciliationRequired;
        };
        let matches = |result: bool| {
            refs.len() == self.intent.changes.len()
                && refs
                    .iter()
                    .map(|r| &r.reference)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == refs.len()
                && self.intent.changes.iter().all(|change| {
                    refs.iter().any(|r| {
                        r.reference == change.reference
                            && r.value
                                == if result {
                                    change.result.clone()
                                } else {
                                    change.expected.clone()
                                }
                    })
                })
        };
        if *marker == RefValue::Exact(self.marker_oid.clone()) && matches(true) {
            DurableRefOperationState::Committed
        } else if *marker == RefValue::Absent && !additional_application_evidence && matches(false)
        {
            DurableRefOperationState::AbortedNotApplied
        } else {
            DurableRefOperationState::ReconciliationRequired
        }
    }
}

impl ForgeCore {
    /// Authenticated repository-scoped recovery readback. Reads remain available
    /// while repository mutation is quarantined. Deleted UUIDs require a current
    /// administrator; slug reuse never transfers historical operation access.
    pub fn merge_operation(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        operation_id: Uuid,
    ) -> Result<MergeOperationReadback> {
        if repository_id.is_nil() || operation_id.is_nil() {
            return Err(invalid(
                "full nonnil repository and operation UUIDs are required",
            ));
        }
        self.validate_mutation_process()?;
        self.runtime
            .coordinator
            .with_repositories(&[repository_id], || {
                let state = self.runtime.state.read();
                let binding = self.validate_actor_locked(&state, actor, false)?;
                let account = state
                    .accounts
                    .get(&binding.login)
                    .ok_or_else(|| ForgeError::Unauthenticated("account disappeared".into()))?;
                require_operation_read_access(
                    &state,
                    &binding.login,
                    account.role == UserRole::Admin,
                    repository_id,
                )?;
                let result = self
                    .operation_storage()?
                    .merge_operation_readback(repository_id, operation_id)?;
                if let DurableRefOperationKind::Merge {
                    source_repository_id,
                    ..
                } = &result.operation.intent.operation
                {
                    require_operation_read_access(
                        &state,
                        &binding.login,
                        account.role == UserRole::Admin,
                        *source_repository_id,
                    )?;
                }
                // Time-based credential expiry is checked again before disclosure.
                self.validate_actor_locked(&state, actor, false)?;
                self.validate_mutation_process()?;
                Ok(result)
            })
    }

    /// Repository-scoped durable readback, including historical deleted UUIDs.
    /// Transport adapters must separately authenticate and authorize readers.
    pub fn get_ref_operation(
        &self,
        repository_id: Uuid,
        operation_id: Uuid,
    ) -> Result<DurableRefOperation> {
        self.validate_mutation_process()?;
        self.operation_storage()?
            .get_ref_operation(repository_id, operation_id)
    }

    pub fn get_ref_operation_by_key(
        &self,
        repository_id: Uuid,
        key: &str,
    ) -> Result<Option<DurableRefOperation>> {
        self.validate_mutation_process()?;
        self.operation_storage()?
            .get_ref_operation_by_key(repository_id, key)
    }

    pub fn pending_ref_operation_events(
        &self,
        repository_id: Uuid,
        limit: u32,
    ) -> Result<Vec<RefOperationEvent>> {
        self.validate_mutation_process()?;
        if limit == 0 || limit > 1000 {
            return Err(invalid("outbox limit must be between1 and1000"));
        }
        self.operation_storage()?
            .pending_ref_operation_events(repository_id, limit)
    }

    // Intentionally private until execute_merge owns authentication, continuous
    // qualification/Git custody and independently observed recovery evidence.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "private execute_merge persistence hook; no transport activation"
        )
    )]
    pub(super) fn prepare_ref_operation(
        &self,
        intent: RefOperationIntent,
    ) -> Result<DurableRefOperation> {
        self.validate_mutation_process()?;
        intent.validate()?;
        let storage = self.operation_storage()?;
        self.runtime
            .coordinator
            .with_repositories(&intent.repositories(), || {
                // Replaying immutable recovery custody is a read, including
                // after revocation, archival or catalog deletion. It must not
                // become a new Git dispatch or a fresh qualification decision.
                if let Some(existing) = storage
                    .get_ref_operation_by_key(intent.repository_id, &intent.idempotency_key)?
                {
                    return if existing.intent == intent {
                        Ok(existing)
                    } else {
                        Err(ForgeError::Conflict(
                            "ref operation idempotency key has different immutable input".into(),
                        ))
                    };
                }
                self.require_ordinary_mutation()?;
                let state = self.runtime.state.read();
                require_repository_admissible(&state, intent.repository_id, true)?;
                for id in intent.repositories() {
                    require_repository_admissible(&state, id, id == intent.repository_id)?;
                }
                drop(state);
                storage.prepare_ref_operation(&intent, Utc::now())
            })
    }

    /// Compose the State closure with operation/audit/outbox persistence. The
    /// closure runs only for a newly committed outcome, never on lost replies,
    /// aborted operations or quarantined observations. Shared State is published
    /// only after SQLite commits; on any error its original bytes remain intact.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "private execute_merge persistence hook; no transport activation"
        )
    )]
    pub(super) fn reconcile_ref_operation(
        &self,
        repository_id: Uuid,
        operation_id: Uuid,
        observation: RefOperationObservation,
        on_committed: impl FnOnce(&mut State) -> Result<()>,
    ) -> Result<DurableRefOperation> {
        self.validate_mutation_process()?;
        let storage = self.operation_storage()?;
        let hint = storage.get_ref_operation(repository_id, operation_id)?;
        self.runtime
            .coordinator
            .with_repositories(&hint.intent.repositories(), || {
                self.require_ordinary_mutation()?;
                let mut state = self.runtime.state.write();
                let (operation, proposed) = storage.reconcile_ref_operation(
                    repository_id,
                    operation_id,
                    &observation,
                    &state,
                    on_committed,
                )?;
                if let Some(proposed) = proposed {
                    *state = proposed;
                }
                Ok(operation)
            })
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "private durable delivery hook; no dispatcher activation"
        )
    )]
    pub(super) fn acknowledge_ref_operation_event(
        &self,
        repository_id: Uuid,
        event_id: Uuid,
        receipt: &str,
    ) -> Result<RefOperationEvent> {
        self.validate_mutation_process()?;
        if receipt.len() != 64
            || !receipt
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("delivery receipt must be a full lowercase SHA256"));
        }
        self.runtime
            .coordinator
            .with_repositories(&[repository_id], || {
                self.require_ordinary_mutation()?;
                self.operation_storage()?.acknowledge_ref_operation_event(
                    repository_id,
                    event_id,
                    receipt,
                )
            })
    }

    fn operation_storage(&self) -> Result<&super::storage::SqliteStore> {
        self.runtime.storage.as_ref().ok_or_else(|| {
            ForgeError::WriterUnavailable(
                "durable ref operations require the admitted SQLite store".into(),
            )
        })
    }
}

pub(super) fn canonical_json<T: Serialize>(value: &T) -> Result<String> {
    fn sorted(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect::<std::collections::BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            Value::Array(values) => Value::Array(values.into_iter().map(sorted).collect()),
            other => other,
        }
    }
    serde_json::to_value(value)
        .and_then(|v| serde_json::to_string(&sorted(v)))
        .map_err(|error| ForgeError::Storage(error.to_string()))
}

pub(super) fn sha256(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn validate_value(value: &RefValue) -> Result<()> {
    match value {
        RefValue::Absent => Ok(()),
        RefValue::Exact(oid) => validate_oid(oid),
    }
}

fn validate_oid(oid: &str) -> Result<()> {
    if oid.len() != 40
        || oid.bytes().all(|b| b == b'0')
        || !oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("full nonzero lowercase SHA1 object ID required"));
    }
    Ok(())
}

fn validate_ref(reference: &str) -> Result<()> {
    // Deliberately supported ASCII head/tag subset; reserved marker refs and
    // arbitrary revision syntax must never enter a managed ref transaction.
    if !(reference.starts_with("refs/heads/") || reference.starts_with("refs/tags/"))
        || reference.len() > 1024
        || reference.ends_with('/')
        || reference.ends_with('.')
        || reference.contains("..")
        || reference.contains("//")
        || reference
            .split('/')
            .any(|part| part.starts_with('.') || part.ends_with(".lock"))
        || !reference
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
    {
        return Err(invalid("unsupported full head/tag ref name"));
    }
    Ok(())
}

fn invalid(message: &str) -> ForgeError {
    ForgeError::Validation(message.into())
}

fn require_operation_read_access(
    state: &State,
    login: &str,
    administrator: bool,
    repository_id: Uuid,
) -> Result<()> {
    if administrator {
        return Ok(());
    }
    let repository = state
        .repos
        .values()
        .find(|repository| repository.id == repository_id)
        .ok_or_else(|| {
            ForgeError::Forbidden("current repository read access is required".into())
        })?;
    let access = state.repo_grants.get(&(
        login.to_owned(),
        repository.owner.clone(),
        repository.name.clone(),
    ));
    if repository.private && !access.is_some_and(|grant| grant.access.allows_read()) {
        return Err(ForgeError::Forbidden(
            "current repository read access is required".into(),
        ));
    }
    Ok(())
}
