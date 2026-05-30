//! Append-only, ed25519-only signed launch ledger.
//!
//! Invariants (load-bearing — every autonomous decision creates a signed
//! receipt):
//!   - The store is append-only. There is no update/delete on the public API;
//!     the in-memory [`MemoryLedger`] never mutates a row once written, mirroring
//!     the SQL `BEFORE UPDATE/DELETE` triggers in the fused DB layer.
//!   - [`VerdictLedger::append`] refuses entries signed with stub/HMAC algos —
//!     only `ed25519` is accepted.
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
mod tests {
    use super::*;
    use crate::signing::EdSigningKey;
    use crate::types::{RiskTier, VerdictReceiptRef};
    use chrono::{Duration, Utc};

    fn signed_entry(id: &str, kind: LedgerKind) -> LaunchLedgerEntry {
        let key = EdSigningKey::generate("test-agent");
        let mut e = LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: id.into(),
            kind,
            subject_id: "subj-1".into(),
            repo: Some("owner/repo".into()),
            payload: serde_json::json!({"hello": "world"}),
            recorded_at: Utc::now(),
            actor: "judge.v1".into(),
            signature: Signature::default_unsigned(),
        };
        sign_entry(&mut e, &key);
        e
    }

    #[tokio::test]
    async fn append_and_list_roundtrip() {
        let ledger = MemoryLedger::new();
        let e = signed_entry("evt-1", LedgerKind::VerdictIssued);
        ledger.append(&e).await.unwrap();
        let got = ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "evt-1");
        assert_eq!(got[0].kind, LedgerKind::VerdictIssued);
        assert_eq!(got[0].subject_id, "subj-1");
        assert_eq!(got[0].payload, serde_json::json!({"hello": "world"}));
        assert_eq!(got[0].signature.algo, "ed25519");
    }

    #[tokio::test]
    async fn append_is_idempotent_on_id() {
        let ledger = MemoryLedger::new();
        let e = signed_entry("evt-dup", LedgerKind::VerdictIssued);
        ledger.append(&e).await.unwrap();
        ledger.append(&e).await.unwrap();
        let got = ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(got.len(), 1, "duplicate id must not insert twice");
    }

    #[tokio::test]
    async fn append_refuses_stub_signature() {
        let ledger = MemoryLedger::new();
        let mut e = signed_entry("evt-stub", LedgerKind::VerdictIssued);
        e.signature = Signature::stub();
        let err = ledger.append(&e).await.unwrap_err();
        assert!(err.to_string().contains("stub"), "actual: {err}");
    }

    #[tokio::test]
    async fn append_refuses_hmac_signature() {
        let ledger = MemoryLedger::new();
        let mut e = signed_entry("evt-hmac", LedgerKind::VerdictIssued);
        e.signature = Signature {
            algo: "sha256-hmac-stub".into(),
            key_id: "k".into(),
            value: "0".repeat(64),
        };
        let err = ledger.append(&e).await.unwrap_err();
        assert!(
            err.to_string().contains("sha256-hmac-stub"),
            "actual: {err}"
        );
    }

    /// Append-only invariant: once a row is written, re-appending the same id
    /// (even with a different body) does NOT mutate the stored row. There is no
    /// update/delete API — mirrors the SQL trigger that aborts UPDATE/DELETE.
    #[tokio::test]
    async fn append_only_no_mutation_after_write() {
        let ledger = MemoryLedger::new();
        let original = signed_entry("evt-x", LedgerKind::VerdictIssued);
        ledger.append(&original).await.unwrap();
        // Attempt to "overwrite" with a tampered body under the same id.
        let mut tampered = original.clone();
        tampered.actor = "hacker".into();
        ledger.append(&tampered).await.unwrap(); // no-op
        let got = ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].actor, "judge.v1",
            "row must be immutable after append"
        );
    }

    #[tokio::test]
    async fn verdict_round_trip_signs_and_appends() {
        let ledger = MemoryLedger::new();
        let now = Utc::now();
        let verdict = VibeGateVerdict {
            schema: SchemaTag::new(),
            id: "vgv_abc".into(),
            evidence_pack_id: "ep_1".into(),
            pull_request: Some("!42".into()),
            repo: "owner/repo".into(),
            target_branch: "main".into(),
            head_sha: "a".repeat(40),
            policy_sha: "c".repeat(40),
            evidence_pack_digest: "sha256:deadbeef".into(),
            risk: RiskTier::R2,
            hard_stops: vec![],
            required_reviews: vec![],
            approval_receipts: Vec::<VerdictReceiptRef>::new(),
            decision: GateDecision::AllowMerge,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at: now + Duration::minutes(60),
            created_at: now,
            signature: Signature::stub(),
        };
        let key = EdSigningKey::generate("judge.v1");
        let mut entry = verdict_issued_entry(&verdict, "judge.v1");
        // Before signing, append must refuse (stub algo).
        assert!(ledger.append(&entry).await.is_err());
        sign_entry(&mut entry, &key);
        ledger.append(&entry).await.unwrap();
        let got = ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(got.len(), 1);
        let body = canonical_body_for_signing(&got[0]);
        assert!(
            key.verifier().verify(body.as_bytes(), &got[0].signature),
            "ed25519 signature must verify after round-trip"
        );
    }

    #[tokio::test]
    async fn list_filters_by_kind_and_subject() {
        let ledger = MemoryLedger::new();
        ledger
            .append(&signed_entry("a", LedgerKind::VerdictIssued))
            .await
            .unwrap();
        let mut other = signed_entry("b", LedgerKind::RollbackInitiated);
        other.subject_id = "subj-2".into();
        ledger.append(&other).await.unwrap();

        let verdicts = ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::VerdictIssued),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].id, "a");

        let subj_2 = ledger
            .list(&LedgerFilter {
                subject_id: Some("subj-2".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(subj_2.len(), 1);
        assert_eq!(subj_2[0].id, "b");
    }

    #[tokio::test]
    async fn concurrent_append_no_corruption_with_four_tasks() {
        let ledger = MemoryLedger::new();
        let mut handles = Vec::new();
        for task in 0..4 {
            let ledger = ledger.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..5 {
                    let id = format!("evt-t{task}-{i}");
                    let e = signed_entry(&id, LedgerKind::VerdictIssued);
                    ledger.append(&e).await.expect("concurrent append");
                }
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }
        let got = ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(
            got.len(),
            20,
            "4 tasks * 5 entries should produce exactly 20 rows"
        );
        let unique: std::collections::HashSet<_> = got.iter().map(|e| e.id.clone()).collect();
        assert_eq!(unique.len(), 20, "no duplicate ids must survive");
    }

    #[tokio::test]
    async fn list_empty_filter_match_returns_empty_vec() {
        let ledger = MemoryLedger::new();
        ledger
            .append(&signed_entry("only-one", LedgerKind::VerdictIssued))
            .await
            .unwrap();
        let got = ledger
            .list(&LedgerFilter {
                subject_id: Some("does-not-exist".into()),
                ..Default::default()
            })
            .await
            .expect("empty result must be Ok");
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn list_limit_boundary_zero_and_one() {
        let ledger = MemoryLedger::new();
        for i in 0..5 {
            let e = signed_entry(&format!("evt-{i}"), LedgerKind::VerdictIssued);
            ledger.append(&e).await.unwrap();
        }
        let none = ledger
            .list(&LedgerFilter {
                limit: Some(0),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(none.len(), 0);
        let one = ledger
            .list(&LedgerFilter {
                limit: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn kind_to_str_handles_webhook_received() {
        assert_eq!(kind_to_str(LedgerKind::WebhookReceived), "webhook_received");
        let back = kind_from_str("webhook_received").expect("decodes");
        assert_eq!(back, LedgerKind::WebhookReceived);
        assert_ne!(
            kind_to_str(LedgerKind::WebhookReceived),
            kind_to_str(LedgerKind::HumanDecisionRecorded)
        );
    }

    #[tokio::test]
    async fn append_then_list_with_webhook_received_kind() {
        let ledger = MemoryLedger::new();
        let entry = signed_entry("wh-1", LedgerKind::WebhookReceived);
        ledger.append(&entry).await.expect("append webhook entry");
        let got = ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::WebhookReceived),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "wh-1");
        let human = ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::HumanDecisionRecorded),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(
            human.is_empty(),
            "webhook entries must NOT leak into the human-decision stream"
        );
    }

    #[tokio::test]
    async fn list_returns_err_on_malformed_json_payload() {
        let ledger = MemoryLedger::new();
        let e = signed_entry("evt-bad-json", LedgerKind::VerdictIssued);
        ledger.append(&e).await.unwrap();
        ledger.corrupt_payload_of("evt-bad-json");
        let err = ledger
            .list(&LedgerFilter::default())
            .await
            .expect_err("malformed payload must surface as an Err");
        let msg = format!("{err}");
        assert!(
            msg.contains("decode") || msg.contains("payload"),
            "error should reference payload decoding; got: {msg}"
        );
    }
}
