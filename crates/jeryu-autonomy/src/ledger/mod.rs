//! Append-only, ed25519-only signed launch ledger.
//!
//! Invariants (load-bearing — every autonomous decision creates a signed
//! receipt):
//!   - The store is append-only. There is no update/delete on the public API;
//!     the in-memory [`MemoryLedger`] never mutates a row once written, mirroring
//!     the SQL `BEFORE UPDATE/DELETE` triggers in the fused DB layer.
//!   - [`VerdictLedger::append`] refuses entries signed with unsigned/HMAC
//!     algos — only `ed25519` is accepted.
//!   - `append` is idempotent on `entry.id`: re-appending the same id is a
//!     no-op. Callers mint a fresh id per logical event.
//!
//! Rows are stored as canonical JSON so a corrupted payload surfaces as a clean
//! `Err` on read, not a panic.

use crate::seam::{LedgerFilter, SeamError, SeamResult, VerdictLedger};
use crate::signing::{EdSigningKey, Signature};
use crate::types::{GateDecision, LaunchLedgerEntry, LedgerKind, SchemaTag, VibeGateVerdict};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// In-memory, append-only, signed ledger. Cheap to clone (shared `Arc`).
#[derive(Clone, Default)]
pub struct MemoryLedger {
    // Stored as (id, raw-JSON). Insertion order preserved = recorded_at ASC for
    // monotonic callers; reads re-sort by recorded_at to be safe.
    rows: Arc<Mutex<Vec<(String, String)>>>,
}

impl MemoryLedger {
    pub fn new() -> Self {
        Self::default()
    }

    fn validate_signature(entry: &LaunchLedgerEntry) -> SeamResult<()> {
        match entry.signature.algo.as_str() {
            "ed25519" => Ok(()),
            other => Err(SeamError::new(
                "ledger",
                format!(
                    "refusing to append entry '{}' signed with algo '{other}'; \
                     only ed25519 is accepted (Law: every decision is signed)",
                    entry.id
                ),
            )),
        }
    }
}

#[async_trait]
impl VerdictLedger for MemoryLedger {
    async fn append(&self, entry: &LaunchLedgerEntry) -> SeamResult<()> {
        Self::validate_signature(entry)?;
        let raw = serde_json::to_string(entry)
            .map_err(|e| SeamError::new("ledger", format!("serialize entry: {e}")))?;
        let mut rows = self.rows.lock().unwrap();
        // Idempotent on id: INSERT OR IGNORE. Never mutate an existing row.
        if rows.iter().any(|(id, _)| id == &entry.id) {
            return Ok(());
        }
        rows.push((entry.id.clone(), raw));
        Ok(())
    }

    async fn list(&self, filter: &LedgerFilter) -> SeamResult<Vec<LaunchLedgerEntry>> {
        let rows = self.rows.lock().unwrap();
        let mut out: Vec<LaunchLedgerEntry> = Vec::new();
        for (_, raw) in rows.iter() {
            // A malformed row surfaces as Err, not a panic (disk-corruption /
            // out-of-band writer case).
            let entry: LaunchLedgerEntry = serde_json::from_str(raw)
                .map_err(|e| SeamError::new("ledger", format!("decode payload: {e}")))?;
            if let Some(k) = filter.kind
                && entry.kind != k
            {
                continue;
            }
            if let Some(s) = &filter.subject_id
                && &entry.subject_id != s
            {
                continue;
            }
            if let Some(r) = &filter.repo
                && entry.repo.as_deref() != Some(r.as_str())
            {
                continue;
            }
            out.push(entry);
        }
        out.sort_by_key(|e| e.recorded_at);
        if let Some(limit) = filter.limit {
            out.truncate(limit.max(0) as usize);
        }
        Ok(out)
    }
}

impl MemoryLedger {
    /// Test/inspection hook: corrupt a stored row's JSON to simulate disk
    /// corruption. Used to prove `list` returns `Err` rather than panicking.
    #[cfg(test)]
    pub(crate) fn corrupt_payload_of(&self, id: &str) {
        let mut rows = self.rows.lock().unwrap();
        if let Some(slot) = rows.iter_mut().find(|(rid, _)| rid == id) {
            slot.1 = "{not valid json".to_string();
        }
    }
}

/// Build an unsigned [`LaunchLedgerEntry`] recording that a verdict was issued.
/// Callers must [`sign_entry`] and then `append`. The fusion path stays pure —
/// this helper lives here so persistence/signing don't leak into `judge`.
pub fn verdict_issued_entry(verdict: &VibeGateVerdict, actor: &str) -> LaunchLedgerEntry {
    let kind = match verdict.decision {
        GateDecision::AllowMerge => LedgerKind::VerdictIssued,
        GateDecision::RequireHuman => LedgerKind::HumanEscalationRequested,
        GateDecision::Reject => LedgerKind::VerdictIssued,
    };
    let payload = serde_json::to_value(verdict).expect("VibeGateVerdict serializes to JSON value");
    LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: format!("ll_{}", verdict.id),
        kind,
        subject_id: verdict.id.clone(),
        repo: Some(verdict.repo.clone()),
        payload,
        recorded_at: verdict.created_at,
        actor: actor.to_string(),
        signature: Signature::default_unsigned(),
    }
}

/// Replace the entry's signature with an ed25519 signature over the canonical
/// body.
pub fn sign_entry(entry: &mut LaunchLedgerEntry, key: &EdSigningKey) {
    let body = canonical_body_for_signing(entry);
    entry.signature = key.sign_raw(body.as_bytes());
}

/// Deterministic concatenation pinning the field order (serde_json is not
/// canonical).
pub fn canonical_body_for_signing(e: &LaunchLedgerEntry) -> String {
    let payload_str =
        serde_json::to_string(&e.payload).expect("serde_json::Value serializes to string");
    format!(
        "{}|{}|{}|{}|{}|{}",
        e.id,
        kind_to_str(e.kind),
        e.subject_id,
        e.repo.as_deref().unwrap_or(""),
        e.recorded_at.to_rfc3339(),
        payload_str
    )
}

pub fn kind_to_str(k: LedgerKind) -> &'static str {
    match k {
        LedgerKind::IntentDeclared => "intent_declared",
        LedgerKind::LeaseIssued => "lease_issued",
        LedgerKind::LeaseExpired => "lease_expired",
        LedgerKind::EvidencePackCreated => "evidence_pack_created",
        LedgerKind::ReviewStarted => "review_started",
        LedgerKind::ReviewCompleted => "review_completed",
        LedgerKind::VerdictIssued => "verdict_issued",
        LedgerKind::MergePassportIssued => "merge_passport_issued",
        LedgerKind::MergePassportConsumed => "merge_passport_consumed",
        LedgerKind::MergePassportInvalidated => "merge_passport_invalidated",
        LedgerKind::ReleasePassportIssued => "release_passport_issued",
        LedgerKind::DeploymentStarted => "deployment_started",
        LedgerKind::DeploymentPromoted => "deployment_promoted",
        LedgerKind::RollbackInitiated => "rollback_initiated",
        LedgerKind::RollbackCompleted => "rollback_completed",
        LedgerKind::HumanEscalationRequested => "human_escalation_requested",
        LedgerKind::HumanDecisionRecorded => "human_decision_recorded",
        LedgerKind::WebhookReceived => "webhook_received",
        LedgerKind::AutonomyPackEditProposed => "autonomy_pack_edit_proposed",
        LedgerKind::AutonomyPackEditMerged => "autonomy_pack_edit_merged",
        LedgerKind::KillBellEngaged => "kill_bell_engaged",
        LedgerKind::KillBellResumed => "kill_bell_resumed",
    }
}

pub fn kind_from_str(s: &str) -> Result<LedgerKind, String> {
    Ok(match s {
        "intent_declared" => LedgerKind::IntentDeclared,
        "lease_issued" => LedgerKind::LeaseIssued,
        "lease_expired" => LedgerKind::LeaseExpired,
        "evidence_pack_created" => LedgerKind::EvidencePackCreated,
        "review_started" => LedgerKind::ReviewStarted,
        "review_completed" => LedgerKind::ReviewCompleted,
        "verdict_issued" => LedgerKind::VerdictIssued,
        "merge_passport_issued" => LedgerKind::MergePassportIssued,
        "merge_passport_consumed" => LedgerKind::MergePassportConsumed,
        "merge_passport_invalidated" => LedgerKind::MergePassportInvalidated,
        "release_passport_issued" => LedgerKind::ReleasePassportIssued,
        "deployment_started" => LedgerKind::DeploymentStarted,
        "deployment_promoted" => LedgerKind::DeploymentPromoted,
        "rollback_initiated" => LedgerKind::RollbackInitiated,
        "rollback_completed" => LedgerKind::RollbackCompleted,
        "human_escalation_requested" => LedgerKind::HumanEscalationRequested,
        "human_decision_recorded" => LedgerKind::HumanDecisionRecorded,
        "webhook_received" => LedgerKind::WebhookReceived,
        "autonomy_pack_edit_proposed" => LedgerKind::AutonomyPackEditProposed,
        "autonomy_pack_edit_merged" => LedgerKind::AutonomyPackEditMerged,
        "kill_bell_engaged" => LedgerKind::KillBellEngaged,
        "kill_bell_resumed" => LedgerKind::KillBellResumed,
        other => return Err(format!("unknown launch_ledger kind: {other}")),
    })
}

#[cfg(test)]
mod tests;
