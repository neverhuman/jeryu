//! Owner: Evidence Gate / autonomy control plane
//! Proof: `cargo test -p jeryu -- autonomy::ledger`
//! Invariants:
//!   - Table is append-only. The Rust API has no update/delete; the SQL
//!     layer enforces it with `BEFORE UPDATE` and `BEFORE DELETE` triggers
//!     installed by `db/state.rs::migrate`.
//!   - `append()` refuses entries signed with stub/HMAC algos. The brainstorm
//!     (tip1 Law 10, tip7/8/9 invariant "every autonomous decision creates a
//!     signed receipt") requires real cryptographic signatures.
//!   - `append()` is idempotent on `entry.id` — re-appending the same id is a
//!     no-op. Callers must mint a fresh id (e.g. uuid v7) per logical event.
//!
//! Wave 11.A: the SQL queries that used to live here moved to
//! `src/db/autonomy_repo.rs`. `SqlLedger` is now a thin wrapper over
//! `AutonomyRepo` so callers don't import `sqlx::` to read or write the
//! ledger. Public API is unchanged.

use anyhow::Result;

use crate::db::AnyPool;
use crate::db::autonomy_repo::{AutonomyRepo, LedgerFilter as RepoLedgerFilter};

use super::signing::{EdSigningKey, Signature};
use super::types::{LaunchLedgerEntry, LedgerKind, SchemaTag, VibeGateVerdict};

#[derive(Debug, Clone)]
pub struct SqlLedger {
    repo: AutonomyRepo,
}

#[derive(Debug, Clone, Default)]
pub struct LedgerFilter {
    pub kind: Option<LedgerKind>,
    pub subject_id: Option<String>,
    pub repo: Option<String>,
    pub limit: Option<i64>,
}

impl LedgerFilter {
    fn to_repo(&self) -> RepoLedgerFilter {
        RepoLedgerFilter {
            kind: self.kind,
            subject_id: self.subject_id.clone(),
            repo: self.repo.clone(),
            limit: self.limit,
        }
    }
}

impl SqlLedger {
    pub fn new(pool: AnyPool) -> Self {
        Self {
            repo: AutonomyRepo::new(pool),
        }
    }

    /// Append one entry. Refuses stub/HMAC signatures (Law 10 in tip1).
    /// Idempotent: same `entry.id` re-appended is a no-op (`INSERT OR IGNORE`).
    pub async fn append(&self, entry: &LaunchLedgerEntry) -> Result<()> {
        self.repo.ledger_append(entry).await
    }

    /// Return entries matching the filter, oldest first.
    pub async fn list(&self, filter: &LedgerFilter) -> Result<Vec<LaunchLedgerEntry>> {
        self.repo.ledger_list(&filter.to_repo()).await
    }
}

/// Build an unsigned `LaunchLedgerEntry` recording that a verdict was issued.
/// Callers must `sign_with(&key)` and then `SqlLedger::append(&entry)`.
/// `judge()` stays pure — this helper lives here so the persistence and
/// signing concerns don't leak into the policy fusion path.
pub fn verdict_issued_entry(verdict: &VibeGateVerdict, actor: &str) -> LaunchLedgerEntry {
    let kind = match verdict.decision {
        crate::autonomy::types::GateDecision::AllowMerge => LedgerKind::VerdictIssued,
        crate::autonomy::types::GateDecision::RequireHuman => LedgerKind::HumanEscalationRequested,
        crate::autonomy::types::GateDecision::Reject => LedgerKind::VerdictIssued,
    };
    // `VibeGateVerdict` is a controlled type with only JSON-safe fields, so
    // serialization cannot fail in practice. Using `expect` (rather than
    // silently substituting `Value::Null`) makes any future schema mistake
    // a loud panic in tests instead of a corrupted ledger row.
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
        // Unsigned marker — caller must invoke `sign_entry(&mut, &key)`
        // before `SqlLedger::append`, which refuses any non-ed25519 algo.
        signature: Signature::default_unsigned(),
    }
}

/// Replace the entry's signature with an ed25519 signature over the
/// canonical-JSON body (kind + subject_id + recorded_at + payload).
pub fn sign_entry(entry: &mut LaunchLedgerEntry, key: &EdSigningKey) {
    let body = canonical_body_for_signing(entry);
    entry.signature = key.sign_raw(body.as_bytes());
}

fn canonical_body_for_signing(e: &LaunchLedgerEntry) -> String {
    // Deterministic concatenation; serde_json::to_string is not canonical so we
    // pin the field order ourselves.
    //
    // `serde_json::Value` always serializes (it is the JSON DOM); `expect`
    // here turns any future invariant violation into a loud panic instead
    // of a silently-different signing body that would mismatch on verify.
    let payload_str =
        serde_json::to_string(&e.payload).expect("serde_json::Value serializes to string");
    format!(
        "{}|{}|{}|{}|{}|{}",
        e.id,
        kind_to_str(e.kind),
        e.subject_id,
        // No `repo` set → empty string in the canonical body. This is the
        // documented semantic for entries that are not repo-scoped (e.g.
        // global kill-bell events); it is NOT an error fallback.
        e.repo.as_deref().unwrap_or(""),
        e.recorded_at.to_rfc3339(),
        payload_str
    )
}

fn kind_to_str(k: LedgerKind) -> &'static str {
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

#[cfg(test)]
fn kind_from_str(s: &str) -> Result<LedgerKind> {
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
        other => anyhow::bail!("unknown launch_ledger kind: {other}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::EdSigningKey;
    use crate::db::autonomy_repo::fresh_autonomy_pool;
    use chrono::Utc;

    async fn fresh_db() -> AnyPool {
        // Test fixture has moved to `crate::db::autonomy_repo`; mirror
        // the old API so existing tests keep their shape.
        fresh_autonomy_pool().await
    }

    fn signed_entry(id: &str, kind: LedgerKind) -> LaunchLedgerEntry {
        let key = EdSigningKey::generate("test-agent");
        let body = format!("{id}|{:?}", kind);
        let sig = key.sign_raw(body.as_bytes());
        LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: id.into(),
            kind,
            subject_id: "subj-1".into(),
            repo: Some("owner/repo".into()),
            payload: serde_json::json!({"hello": "world"}),
            recorded_at: Utc::now(),
            actor: "judge.v1".into(),
            signature: sig,
        }
    }

    #[tokio::test]
    async fn append_and_list_roundtrip() {
        let ledger = SqlLedger::new(fresh_db().await);
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
        let ledger = SqlLedger::new(fresh_db().await);
        let e = signed_entry("evt-dup", LedgerKind::VerdictIssued);
        ledger.append(&e).await.unwrap();
        ledger.append(&e).await.unwrap();
        let got = ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(got.len(), 1, "duplicate id must not insert twice");
    }

    #[tokio::test]
    async fn append_refuses_stub_signature() {
        let ledger = SqlLedger::new(fresh_db().await);
        let mut e = signed_entry("evt-stub", LedgerKind::VerdictIssued);
        e.signature = Signature::stub();
        let err = ledger.append(&e).await.unwrap_err();
        assert!(err.to_string().contains("stub"), "actual: {err}");
    }

    #[tokio::test]
    async fn append_refuses_hmac_signature() {
        let ledger = SqlLedger::new(fresh_db().await);
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

    #[tokio::test]
    async fn append_only_trigger_blocks_update() {
        // This test asserts the trigger fires on raw UPDATE/DELETE. Since
        // the test bypasses the public API to corrupt the pool directly,
        // it still needs a raw query — routed through `crate::db::raw_query`
        // so this file does not import `sqlx::`.
        use crate::db::raw_query;
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        ledger
            .append(&signed_entry("evt-x", LedgerKind::VerdictIssued))
            .await
            .unwrap();
        let res = raw_query("UPDATE launch_ledger SET actor='hacker' WHERE id='evt-x'")
            .execute(&pool)
            .await;
        assert!(res.is_err(), "trigger must abort UPDATE");
        let res = raw_query("DELETE FROM launch_ledger WHERE id='evt-x'")
            .execute(&pool)
            .await;
        assert!(res.is_err(), "trigger must abort DELETE");
    }

    #[tokio::test]
    async fn verdict_round_trip_signs_and_appends() {
        use crate::autonomy::types::{
            GateDecision, RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
        };
        use chrono::Duration;

        let ledger = SqlLedger::new(fresh_db().await);
        let now = Utc::now();
        let verdict = VibeGateVerdict {
            schema: SchemaTag::new(),
            id: "vgv_abc".into(),
            evidence_pack_id: "ep_1".into(),
            merge_request: Some("!42".into()),
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
        // After signing, append must succeed.
        ledger.append(&entry).await.unwrap();

        // Verify the entry round-trips and signature still verifies.
        let got = ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(got.len(), 1);
        let body = canonical_body_for_signing(&got[0]);
        assert!(
            key.verifier().verify(body.as_bytes(), &got[0].signature),
            "ed25519 signature must verify after DB round-trip"
        );
    }

    #[tokio::test]
    async fn list_filters_by_kind_and_subject() {
        let ledger = SqlLedger::new(fresh_db().await);
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

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// Concurrent appends from 4 tokio tasks must produce no corruption: every
    /// entry lands exactly once, the append-only triggers stay honored, and
    /// the final list matches the inserted set regardless of interleave order.
    #[tokio::test]
    async fn concurrent_append_no_corruption_with_four_tasks() {
        let ledger = SqlLedger::new(fresh_db().await);
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

    /// list() with a filter that matches no rows must return Ok(empty) — not
    /// a sql error and not a panic.
    #[tokio::test]
    async fn list_empty_filter_match_returns_empty_vec() {
        let ledger = SqlLedger::new(fresh_db().await);
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
        assert!(got.is_empty(), "no rows must produce an empty Vec");
    }

    /// limit=0 must return zero rows; limit=1 must return one even with many
    /// in the table. Boundary check for the LIMIT clause path.
    #[tokio::test]
    async fn list_limit_boundary_zero_and_one() {
        let ledger = SqlLedger::new(fresh_db().await);
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

    /// Wave 10 mint — the WebhookReceived variant must round-trip through
    /// the snake_case mapper in BOTH directions. If either direction
    /// breaks, audit replay over a webhook trail goes blank.
    #[test]
    fn kind_to_str_handles_webhook_received() {
        assert_eq!(kind_to_str(LedgerKind::WebhookReceived), "webhook_received");
        let back = kind_from_str("webhook_received").expect("decodes");
        assert_eq!(back, LedgerKind::WebhookReceived);
        // Disjoint from the human-decision variant (the old, reused kind).
        assert_ne!(
            kind_to_str(LedgerKind::WebhookReceived),
            kind_to_str(LedgerKind::HumanDecisionRecorded)
        );
    }

    /// Wave 10 mint — round-trip a `WebhookReceived` entry through the SQL
    /// ledger so `replay_subject` / `autonomy replay` can pick it up as a
    /// dedicated event class. Append refuses stub signatures, so the entry
    /// is signed with a fresh ed25519 key first.
    #[tokio::test]
    async fn append_then_list_with_webhook_received_kind() {
        let ledger = SqlLedger::new(fresh_db().await);
        let entry = signed_entry("wh-1", LedgerKind::WebhookReceived);
        ledger.append(&entry).await.expect("append webhook entry");

        let got = ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::WebhookReceived),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(got.len(), 1, "filter by WebhookReceived must hit the row");
        assert_eq!(got[0].id, "wh-1");
        assert_eq!(got[0].kind, LedgerKind::WebhookReceived);
        // Storing the new variant must not pollute the legacy human-decision
        // bucket: filter by HumanDecisionRecorded must be empty.
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

    /// A row with malformed JSON in the payload column must surface as a
    /// clean Result::Err on read, not a panic. Append goes through the
    /// public API; we corrupt the payload directly with raw SQL (routed
    /// through the db boundary helper) to simulate disk corruption or an
    /// out-of-band writer.
    #[tokio::test]
    async fn list_returns_err_on_malformed_json_payload() {
        use crate::db::raw_query;
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let e = signed_entry("evt-bad-json", LedgerKind::VerdictIssued);
        ledger.append(&e).await.unwrap();
        // The append-only triggers block UPDATE, so we delete-via-trigger-error
        // is impossible; instead, insert a second row via a new id with raw
        // INSERT bypassing the Rust serializer. That keeps the trigger happy
        // while still corrupting the payload field.
        raw_query(
            "INSERT INTO launch_ledger
                 (id, kind, subject_id, repo, actor, payload,
                  signature_algo, signature_key_id, signature_value, recorded_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("evt-corrupt")
        .bind("verdict_issued")
        .bind("s")
        .bind::<Option<&str>>(None)
        .bind("a")
        .bind("{not valid json")
        .bind("ed25519")
        .bind("k")
        .bind("0".repeat(128))
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
        let err = ledger
            .list(&LedgerFilter::default())
            .await
            .expect_err("malformed payload must surface as an Err");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("decode") || msg.contains("payload") || msg.contains("json"),
            "error message should reference payload decoding; got: {msg}"
        );
    }
}
