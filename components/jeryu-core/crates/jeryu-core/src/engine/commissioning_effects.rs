//! Core-owned restoration state. A completed step never releases the operation
//! barrier. The unsigned scope must come from an installed commissioning verifier.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ReviewActorBinding;
use crate::{ForgeError, Result};

pub const COMMISSIONING_TARGETS: [&str; 13] = [
    "host-ci-sandbox",
    "host-ci-boundary-abort",
    "host-ci-boundary-promote",
    "host-ci-boundary-preflight",
    "host-ci-inputs",
    "native-build-tools-install",
    "native-runtime.sh",
    "native-build-tools.lock.json",
    "splitctl",
    "host-ci-publisher",
    "host-ci-sandbox.config.json",
    "host-ci-publisher.config.json",
    "native-build-tools-installer.config.json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommissioningRestoreKind {
    RestorePreAcceptanceP,
    RecoverAcceptedCForward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningSource {
    pub commit_sha: String,
    pub tree_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningRestoreRequest {
    pub expected_contract_sha256: String,
    pub expected_snapshot_sha256: String,
    pub idempotency_key: String,
    pub kind: CommissioningRestoreKind,
    pub before_manifest_sha256: String,
    pub desired_manifest_sha256: String,
}

/// This is a verified contract projection, not a client-selected enrollment.
/// P is the ordinary predecessor; the installed entry is expired bootstrap B.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningRestoreScope {
    pub contract_id: Uuid,
    pub repository_id: Uuid,
    pub backing_pair_id: Uuid,
    pub request: CommissioningRestoreRequest,
    pub operator: ReviewActorBinding,
    pub p: CommissioningSource,
    pub b: CommissioningSource,
    pub c: CommissioningSource,
    pub observed_main: CommissioningSource,
    pub installation_inventory_sha256: String,
    pub recovery_inventory_sha256: String,
    pub desired_present: [bool; 13],
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    content = "target",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CommissioningStep {
    CreateStage,
    PrepareTarget(u8),
    PreparedTarget(u8),
    ApplyTarget(u8),
    ReadbackTarget(u8),
    FinalReadback,
    CompletionDurable,
}

impl CommissioningStep {
    pub fn target_name(self) -> Result<Option<&'static str>> {
        match self {
            Self::PrepareTarget(i)
            | Self::PreparedTarget(i)
            | Self::ApplyTarget(i)
            | Self::ReadbackTarget(i) => COMMISSIONING_TARGETS
                .get(usize::from(i))
                .copied()
                .map(Some)
                .ok_or_else(|| invalid("unknown recovery target")),
            _ => Ok(None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningRevision {
    pub revision: u64,
    pub record_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningOperationAddress {
    pub repository_id: Uuid,
    pub contract_id: Uuid,
    pub operation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningStepRequest {
    pub expected: CommissioningRevision,
    pub step: CommissioningStep,
    pub local_custody_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommissioningEffectOutcome {
    Verified,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommissioningRecordingAuthority {
    CurrentOperator,
    RecoveryRecorder,
}

/// Receiving bytes, not caller-provided digests. Actual evidence interpretation
/// belongs to the installed verifier; the journal hashes and retains these bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningCompletionRequest {
    pub expected: CommissioningRevision,
    pub step_id: Uuid,
    pub outcome: CommissioningEffectOutcome,
    pub evidence: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningStepCompletion {
    pub expected: CommissioningRevision,
    pub outcome: CommissioningEffectOutcome,
    pub evidence: Vec<u8>,
    pub evidence_sha256: String,
    pub recorder: ReviewActorBinding,
    pub recording_authority: CommissioningRecordingAuthority,
    pub recorded_at: DateTime<Utc>,
    /// False for expiry, changed/revoked original authority, or failure/unknown.
    /// Recording a claimed successful outcome cannot turn this field true.
    pub authorizes_progress: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningAdmittedStep {
    pub id: Uuid,
    pub expected: CommissioningRevision,
    pub step: CommissioningStep,
    pub local_custody_sha256: String,
    pub admitted_at: DateTime<Utc>,
    pub completion: Option<CommissioningStepCompletion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommissioningRestoreOperation {
    pub id: Uuid,
    pub scope: CommissioningRestoreScope,
    pub reserved_at: DateTime<Utc>,
    pub last_recorded_at: DateTime<Utc>,
    pub current: CommissioningRevision,
    pub steps: Vec<CommissioningAdmittedStep>,
    pub recovery_required: bool,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum CommissioningJournalEvent {
    Reserved {
        scope: Box<CommissioningRestoreScope>,
    },
    StepAdmitted {
        step: CommissioningAdmittedStep,
    },
    StepCompleted {
        step_id: Uuid,
        completion: CommissioningStepCompletion,
    },
    Closed {
        final_evidence: Vec<u8>,
        final_evidence_sha256: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommissioningJournalRecord {
    pub operation_id: Uuid,
    pub revision: u64,
    pub previous_sha256: String,
    pub recorded_at: DateTime<Utc>,
    pub actor: ReviewActorBinding,
    pub body: CommissioningJournalEvent,
}

impl CommissioningRestoreScope {
    pub(super) fn validate(&self) -> Result<()> {
        if self.contract_id.is_nil()
            || self.repository_id.is_nil()
            || self.backing_pair_id.is_nil()
            || self.expires_at <= self.not_before
            || self.expires_at - self.not_before > Duration::seconds(7200)
        {
            return Err(invalid("invalid fixed commissioning scope"));
        }
        super::publisher_enrollment::actor_shape(&self.operator)?;
        self.request.validate()?;
        for digest in [
            &self.installation_inventory_sha256,
            &self.recovery_inventory_sha256,
        ] {
            digest_shape(digest)?;
        }
        for source in [&self.p, &self.b, &self.c, &self.observed_main] {
            if !super::required_attempts::full_oid(&source.commit_sha)
                || !super::required_attempts::full_oid(&source.tree_sha)
                || source.commit_sha.len() != source.tree_sha.len()
                || source.commit_sha.len() != self.p.commit_sha.len()
            {
                return Err(invalid("invalid exact commissioning Git source"));
            }
        }
        if self.p.commit_sha == self.b.commit_sha
            || self.p.commit_sha == self.c.commit_sha
            || self.b.commit_sha == self.c.commit_sha
        {
            return Err(invalid("P/B/C must be distinct exact source identities"));
        }
        let expected = match self.request.kind {
            CommissioningRestoreKind::RestorePreAcceptanceP => &self.b,
            CommissioningRestoreKind::RecoverAcceptedCForward => &self.c,
        };
        if self.observed_main != *expected {
            return Err(conflict(
                "restoration direction disagrees with observed accepted Git",
            ));
        }
        Ok(())
    }

    pub(super) fn require_window(&self, now: DateTime<Utc>) -> Result<()> {
        if now < self.not_before || now >= self.expires_at {
            Err(conflict("commissioning effect is outside its fixed window"))
        } else {
            Ok(())
        }
    }

    fn sequence(&self) -> Vec<CommissioningStep> {
        let mut steps = vec![CommissioningStep::CreateStage];
        for i in 0..13u8 {
            steps.push(CommissioningStep::PrepareTarget(i));
            if self.desired_present[usize::from(i)] {
                steps.push(CommissioningStep::PreparedTarget(i));
            }
            steps.push(CommissioningStep::ApplyTarget(i));
            steps.push(CommissioningStep::ReadbackTarget(i));
        }
        steps.extend([
            CommissioningStep::FinalReadback,
            CommissioningStep::CompletionDurable,
        ]);
        steps
    }
}

impl CommissioningRestoreRequest {
    pub(super) fn validate(&self) -> Result<()> {
        if self.expected_contract_sha256.is_empty()
            || self.expected_snapshot_sha256.is_empty()
            || self.before_manifest_sha256.is_empty()
            || self.desired_manifest_sha256.is_empty()
            || self.idempotency_key.is_empty()
        {
            return Err(ForgeError::PreconditionRequired(
                "exact commissioning identities and idempotency key are required".into(),
            ));
        }
        for digest in [
            &self.expected_contract_sha256,
            &self.expected_snapshot_sha256,
            &self.before_manifest_sha256,
            &self.desired_manifest_sha256,
        ] {
            digest_shape(digest)?;
        }
        if !super::required_attempts::safe_text(&self.idempotency_key, 256) {
            return Err(invalid("invalid commissioning idempotency key"));
        }
        Ok(())
    }
}

impl CommissioningRevision {
    pub(super) fn validate(&self) -> Result<()> {
        if self.revision == 0 || self.record_sha256.is_empty() {
            return Err(ForgeError::PreconditionRequired(
                "exact commissioning revision and record hash are required".into(),
            ));
        }
        if self.revision > 9_007_199_254_740_991 {
            return Err(invalid("commissioning revision exceeds exact range"));
        }
        digest_shape(&self.record_sha256)
    }
}

impl CommissioningRestoreOperation {
    pub fn blocks_admission(&self) -> bool {
        self.closed_at.is_none()
    }

    pub(super) fn next_step(&self, now: DateTime<Utc>) -> Result<Option<CommissioningStep>> {
        self.scope.require_window(now)?;
        if self.recovery_required
            || self.closed_at.is_some()
            || self.steps.last().is_some_and(|s| s.completion.is_none())
        {
            return Err(conflict(
                "recovery, closure or unresolved effect prevents a new step",
            ));
        }
        Ok(self.scope.sequence().get(self.steps.len()).copied())
    }
}

pub(super) fn apply_record(
    prior: Option<CommissioningRestoreOperation>,
    record: &CommissioningJournalRecord,
) -> Result<CommissioningRestoreOperation> {
    if record.operation_id.is_nil() {
        return Err(invalid("nil commissioning operation"));
    }
    super::publisher_enrollment::actor_shape(&record.actor)?;
    let mut operation = match (&record.body, prior) {
        (CommissioningJournalEvent::Reserved { scope }, None) => {
            scope.validate()?;
            scope.require_window(record.recorded_at)?;
            if record.revision != 1
                || record.previous_sha256 != "0".repeat(64)
                || record.actor != scope.operator
            {
                return Err(conflict("invalid reservation record"));
            }
            CommissioningRestoreOperation {
                id: record.operation_id,
                scope: scope.as_ref().clone(),
                reserved_at: record.recorded_at,
                last_recorded_at: record.recorded_at,
                current: CommissioningRevision {
                    revision: 0,
                    record_sha256: "0".repeat(64),
                },
                steps: Vec::new(),
                recovery_required: false,
                closed_at: None,
            }
        }
        (CommissioningJournalEvent::Reserved { .. }, Some(_)) | (_, None) => {
            return Err(conflict("missing or repeated commissioning reservation"));
        }
        (_, Some(operation)) => operation,
    };
    if record.operation_id != operation.id
        || Some(record.revision) != operation.current.revision.checked_add(1)
        || record.previous_sha256 != operation.current.record_sha256
        || operation.closed_at.is_some()
        || record.recorded_at < operation.last_recorded_at
    {
        return Err(conflict(
            "commissioning record chain or terminal state differs",
        ));
    }
    match &record.body {
        CommissioningJournalEvent::Reserved { .. } => (),
        CommissioningJournalEvent::StepAdmitted { step } => {
            digest_shape(&step.local_custody_sha256)?;
            if record.actor != operation.scope.operator
                || step.id.is_nil()
                || operation.steps.iter().any(|prior| prior.id == step.id)
                || step.expected != operation.current
                || step.completion.is_some()
                || step.admitted_at != record.recorded_at
                || operation.next_step(record.recorded_at)? != Some(step.step)
            {
                return Err(conflict(
                    "step differs from the exact admitted sequence or operator",
                ));
            }
            operation.steps.push(step.clone());
        }
        CommissioningJournalEvent::StepCompleted {
            step_id,
            completion,
        } => {
            evidence_shape(&completion.evidence)?;
            if completion.expected != operation.current
                || completion.evidence_sha256 != bytes_hash(&completion.evidence)
                || completion.recorder != record.actor
                || completion.recorded_at != record.recorded_at
            {
                return Err(conflict("completion receiving evidence or actor differs"));
            }
            let step = operation
                .steps
                .last_mut()
                .ok_or_else(|| conflict("no admitted step"))?;
            if step.id != *step_id
                || step.completion.is_some()
                || record.recorded_at < step.admitted_at
            {
                return Err(conflict(
                    "completion does not target the current unresolved step",
                ));
            }
            let authorizes_progress = completion.recording_authority
                == CommissioningRecordingAuthority::CurrentOperator
                && completion.recorder == operation.scope.operator
                && completion.outcome == CommissioningEffectOutcome::Verified
                && operation.scope.require_window(record.recorded_at).is_ok()
                && !operation.recovery_required;
            if completion.authorizes_progress != authorizes_progress {
                return Err(conflict(
                    "stored completion overclaims current effect authority",
                ));
            }
            operation.recovery_required |= !authorizes_progress;
            step.completion = Some(completion.clone());
        }
        CommissioningJournalEvent::Closed {
            final_evidence,
            final_evidence_sha256,
        } => {
            evidence_shape(final_evidence)?;
            if record.actor != operation.scope.operator
                || operation.next_step(record.recorded_at)?.is_some()
                || *final_evidence_sha256 != bytes_hash(final_evidence)
            {
                return Err(conflict(
                    "unverified or incomplete operation cannot release its barrier",
                ));
            }
            operation.closed_at = Some(record.recorded_at);
        }
    }
    operation.current = CommissioningRevision {
        revision: record.revision,
        record_sha256: super::commissioning_canonical::payload_sha256(
            "jeryu.commissioning-journal-record/v1",
            record,
        )?,
    };
    operation.last_recorded_at = record.recorded_at;
    Ok(operation)
}

pub(super) fn bytes_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
pub(super) fn evidence_shape(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > 64 * 1024 {
        Err(invalid(
            "commissioning evidence must contain 1..65536 receiving bytes",
        ))
    } else {
        Ok(())
    }
}
pub(super) fn digest_shape(value: &str) -> Result<()> {
    if super::required_attempts::full_sha256(value) {
        Ok(())
    } else {
        Err(invalid(
            "commissioning digest must be lowercase full SHA-256",
        ))
    }
}
pub(super) fn invalid(message: &str) -> ForgeError {
    ForgeError::Validation(message.into())
}
pub(super) fn conflict(message: &str) -> ForgeError {
    ForgeError::Conflict(message.into())
}
