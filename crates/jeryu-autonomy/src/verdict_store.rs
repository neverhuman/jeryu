//! Verdict persistence (the live-verdict projection the daemon polls).
//!
//! Invariants:
//!   - `save()` is idempotent on `verdict.id`.
//!   - Before inserting a new verdict for an existing (repo, pull_request) pair,
//!     every prior non-superseded row for that pair is marked superseded. This
//!     keeps `load_latest` cheap and gives `list_active` a single boolean.
//!   - `body_json` is the source of truth: the full [`VibeGateVerdict`]
//!     round-trips losslessly.
//!   - This store does NOT enforce signing — unlike the ledger. The daemon may
//!     persist unsigned verdicts here for replay/debug.

use crate::seam::{SeamResult, VerdictStore};
use crate::types::VibeGateVerdict;
use crate::types::GateDecision;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

/// One persisted row. `body` is the source of truth; `superseded_at` is the
/// single boolean `list_active` filters on.
#[derive(Clone)]
struct Row {
    body: VibeGateVerdict,
    superseded_at: Option<DateTime<Utc>>,
}

/// In-memory verdict store. Cheap to clone (shared `Arc`).
#[derive(Clone, Default)]
pub struct MemoryVerdictStore {
    rows: Arc<Mutex<Vec<Row>>>,
}

impl MemoryVerdictStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl VerdictStore for MemoryVerdictStore {
    async fn save(&self, verdict: &VibeGateVerdict) -> SeamResult<()> {
        let mut rows = self.rows.lock().unwrap();
        // Idempotent on id.
        if rows.iter().any(|r| r.body.id == verdict.id) {
            return Ok(());
        }
        // Supersede prior non-superseded rows for the same (repo, pull_request).
        for r in rows.iter_mut() {
            if r.superseded_at.is_none()
                && r.body.repo == verdict.repo
                && r.body.pull_request == verdict.pull_request
            {
                r.superseded_at = Some(verdict.created_at);
            }
        }
        rows.push(Row { body: verdict.clone(), superseded_at: None });
        Ok(())
    }

    async fn load_latest(
        &self,
        repo: &str,
        pull_request: Option<&str>,
    ) -> SeamResult<Option<VibeGateVerdict>> {
        let rows = self.rows.lock().unwrap();
        let pr = pull_request.map(|s| s.to_string());
        let latest = rows
            .iter()
            .filter(|r| {
                r.superseded_at.is_none() && r.body.repo == repo && r.body.pull_request == pr
            })
            .max_by(|a, b| a.body.created_at.cmp(&b.body.created_at))
            .map(|r| r.body.clone());
        Ok(latest)
    }

    async fn list_active(&self, now: DateTime<Utc>) -> SeamResult<Vec<VibeGateVerdict>> {
        let rows = self.rows.lock().unwrap();
        let mut active: Vec<VibeGateVerdict> = rows
            .iter()
            .filter(|r| {
                r.superseded_at.is_none()
                    && r.body.expires_at > now
                    && r.body.decision != GateDecision::Reject
            })
            .map(|r| r.body.clone())
            .collect();
        active.sort_by_key(|v| v.created_at);
        Ok(active)
    }

    async fn supersede(&self, verdict_id: &str, now: DateTime<Utc>) -> SeamResult<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(r) = rows.iter_mut().find(|r| r.body.id == verdict_id)
            && r.superseded_at.is_none()
        {
            r.superseded_at = Some(now);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::Signature;
    use crate::types::{RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict};
    use chrono::Duration;

    fn mint_verdict(
        repo: &str,
        pr: Option<&str>,
        head_sha_tail: &str,
        decision: GateDecision,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> VibeGateVerdict {
        let head_sha = format!("{head_sha_tail:0>40}");
        let id = format!(
            "vgv_{}_{}",
            created_at.timestamp_millis(),
            &head_sha[head_sha.len().saturating_sub(8)..]
        );
        VibeGateVerdict {
            schema: SchemaTag::new(),
            id,
            evidence_pack_id: "ep_test".into(),
            pull_request: pr.map(|s| s.to_string()),
            repo: repo.into(),
            target_branch: "main".into(),
            head_sha,
            policy_sha: "c".repeat(40),
            evidence_pack_digest: "sha256:deadbeef".into(),
            risk: RiskTier::R2,
            hard_stops: vec![],
            required_reviews: vec![],
            approval_receipts: Vec::<VerdictReceiptRef>::new(),
            decision,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at,
            created_at,
            signature: Signature::stub(),
        }
    }

    #[tokio::test]
    async fn save_then_load_latest_round_trips() {
        let store = MemoryVerdictStore::new();
        let now = Utc::now();
        let v = mint_verdict("owner/repo", Some("!42"), "abc12345", GateDecision::AllowMerge, now, now + Duration::minutes(60));
        store.save(&v).await.unwrap();
        let got = store.load_latest("owner/repo", Some("!42")).await.unwrap().expect("round-trip");
        assert_eq!(got.id, v.id);
        assert_eq!(got.pull_request, v.pull_request);
        assert_eq!(got.decision, GateDecision::AllowMerge);
    }

    #[tokio::test]
    async fn save_is_idempotent_on_id() {
        let store = MemoryVerdictStore::new();
        let now = Utc::now();
        let v = mint_verdict("owner/repo", Some("!1"), "ffff0001", GateDecision::AllowMerge, now, now + Duration::minutes(30));
        store.save(&v).await.unwrap();
        store.save(&v).await.unwrap();
        store.save(&v).await.unwrap();
        let active = store.list_active(now).await.unwrap();
        assert_eq!(active.len(), 1, "same id must not insert twice");
        assert_eq!(active[0].id, v.id);
    }

    #[tokio::test]
    async fn save_supersedes_prior_verdicts_for_same_repo_and_pr() {
        let store = MemoryVerdictStore::new();
        let t0 = Utc::now();
        let v1 = mint_verdict("owner/repo", Some("!9"), "aaaa1111", GateDecision::AllowMerge, t0, t0 + Duration::minutes(60));
        let v2 = mint_verdict("owner/repo", Some("!9"), "bbbb2222", GateDecision::AllowMerge, t0 + Duration::seconds(5), t0 + Duration::minutes(60));
        store.save(&v1).await.unwrap();
        store.save(&v2).await.unwrap();
        let got = store.load_latest("owner/repo", Some("!9")).await.unwrap().expect("latest");
        assert_eq!(got.id, v2.id, "newer save must win");
        let active = store.list_active(t0).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, v2.id);
    }

    #[tokio::test]
    async fn load_latest_returns_none_for_unknown_pair() {
        let store = MemoryVerdictStore::new();
        let now = Utc::now();
        let v = mint_verdict("owner/repo", Some("!1"), "11112222", GateDecision::AllowMerge, now, now + Duration::minutes(60));
        store.save(&v).await.unwrap();
        assert!(store.load_latest("owner/other", Some("!1")).await.unwrap().is_none());
        assert!(store.load_latest("owner/repo", Some("!999")).await.unwrap().is_none());
        // None vs Some("!1") are distinct.
        assert!(store.load_latest("owner/repo", None).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_active_excludes_expired_verdicts() {
        let store = MemoryVerdictStore::new();
        let now = Utc::now();
        let expired = mint_verdict("owner/repo", Some("!1"), "11110000", GateDecision::AllowMerge, now - Duration::minutes(120), now - Duration::minutes(60));
        let live = mint_verdict("owner/repo", Some("!2"), "22220000", GateDecision::AllowMerge, now, now + Duration::minutes(60));
        store.save(&expired).await.unwrap();
        store.save(&live).await.unwrap();
        let active = store.list_active(now).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, live.id);
    }

    #[tokio::test]
    async fn list_active_excludes_rejected_verdicts() {
        let store = MemoryVerdictStore::new();
        let now = Utc::now();
        let allow = mint_verdict("owner/repo", Some("!a"), "aaaa0000", GateDecision::AllowMerge, now, now + Duration::minutes(60));
        let reject = mint_verdict("owner/repo", Some("!r"), "ffff0000", GateDecision::Reject, now, now + Duration::minutes(60));
        let human = mint_verdict("owner/repo", Some("!h"), "cccc0000", GateDecision::RequireHuman, now, now + Duration::minutes(60));
        store.save(&allow).await.unwrap();
        store.save(&reject).await.unwrap();
        store.save(&human).await.unwrap();
        let active = store.list_active(now).await.unwrap();
        let ids: Vec<&str> = active.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(active.len(), 2, "reject must be excluded; got ids={ids:?}");
        assert!(ids.contains(&allow.id.as_str()));
        assert!(ids.contains(&human.id.as_str()));
        assert!(!ids.contains(&reject.id.as_str()));
    }

    #[tokio::test]
    async fn list_active_orders_by_created_at_ascending() {
        let store = MemoryVerdictStore::new();
        let t0 = Utc::now();
        let v_b = mint_verdict("owner/repo", Some("!b"), "bbbb0001", GateDecision::AllowMerge, t0 + Duration::seconds(20), t0 + Duration::minutes(60));
        let v_a = mint_verdict("owner/repo", Some("!a"), "aaaa0001", GateDecision::AllowMerge, t0 + Duration::seconds(10), t0 + Duration::minutes(60));
        let v_c = mint_verdict("owner/repo", Some("!c"), "cccc0001", GateDecision::AllowMerge, t0 + Duration::seconds(30), t0 + Duration::minutes(60));
        store.save(&v_c).await.unwrap();
        store.save(&v_a).await.unwrap();
        store.save(&v_b).await.unwrap();
        let active = store.list_active(t0).await.unwrap();
        assert_eq!(active.len(), 3);
        assert_eq!(active[0].id, v_a.id, "earliest first");
        assert_eq!(active[1].id, v_b.id);
        assert_eq!(active[2].id, v_c.id, "latest last");
    }

    #[tokio::test]
    async fn supersede_marks_row_and_is_idempotent() {
        let store = MemoryVerdictStore::new();
        let now = Utc::now();
        let v = mint_verdict("owner/repo", Some("!1"), "fade0001", GateDecision::AllowMerge, now, now + Duration::minutes(60));
        store.save(&v).await.unwrap();
        assert_eq!(store.list_active(now).await.unwrap().len(), 1);
        store.supersede(&v.id, now + Duration::seconds(5)).await.unwrap();
        assert_eq!(store.list_active(now).await.unwrap().len(), 0);
        store.supersede(&v.id, now + Duration::seconds(10)).await.expect("idempotent");
        store.supersede("vgv_nope", now).await.expect("unknown id is a no-op");
    }

    #[tokio::test]
    async fn body_json_is_source_of_truth_after_round_trip() {
        use crate::types::{ReviewDecision, ReviewerRole};
        let store = MemoryVerdictStore::new();
        let now = Utc::now();
        let mut v = mint_verdict("owner/repo", Some("!42"), "beef0001", GateDecision::AllowMerge, now, now + Duration::minutes(60));
        v.hard_stops = vec!["security:high".into(), "tests:full_required".into()];
        v.approval_receipts = vec![
            VerdictReceiptRef { role: ReviewerRole::Security, agent_id: "reviewer-security.v1".into(), receipt_digest: "sha256:cafe".into(), decision: ReviewDecision::Pass, not_author: true },
            VerdictReceiptRef { role: ReviewerRole::Judge, agent_id: "judge.v1".into(), receipt_digest: "sha256:beef".into(), decision: ReviewDecision::Pass, not_author: true },
        ];
        store.save(&v).await.unwrap();
        let got = store.load_latest("owner/repo", Some("!42")).await.unwrap().expect("loads");
        assert_eq!(got, v, "body must round-trip every field losslessly");
    }

    #[tokio::test]
    async fn concurrent_save_no_corruption_with_four_tasks() {
        let store = MemoryVerdictStore::new();
        let t0 = Utc::now();
        let mut handles = Vec::new();
        for task in 0..4 {
            let store = store.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..5 {
                    let v = mint_verdict(
                        "owner/repo",
                        Some(&format!("!t{task}-{i}")),
                        &format!("t{task}i{i:03}"),
                        GateDecision::AllowMerge,
                        t0 + Duration::milliseconds((task * 100 + i) as i64),
                        t0 + Duration::minutes(60),
                    );
                    store.save(&v).await.expect("concurrent save");
                }
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }
        let active = store.list_active(t0).await.unwrap();
        assert_eq!(active.len(), 20, "4 tasks * 5 distinct verdicts must produce 20 active rows");
        let got = store.load_latest("owner/repo", Some("!t2-3")).await.unwrap().expect("pair exists");
        assert_eq!(got.pull_request.as_deref(), Some("!t2-3"));
    }

    #[tokio::test]
    async fn save_with_unsigned_verdict_succeeds_for_replay_use_case() {
        let store = MemoryVerdictStore::new();
        let now = Utc::now();
        let mut v = mint_verdict("owner/repo", Some("!unsigned"), "0bad0001", GateDecision::AllowMerge, now, now + Duration::minutes(60));
        v.signature = Signature::stub();
        store.save(&v).await.expect("verdict_store accepts unsigned verdicts (replay use case)");
        let got = store.load_latest("owner/repo", Some("!unsigned")).await.unwrap().expect("round-trip");
        assert_eq!(got.signature.algo, "stub");
    }
}
