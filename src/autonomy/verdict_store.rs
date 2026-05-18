//! Owner: Evidence Gate / autonomous-delivery daemon (Wave 7.A)
//! Proof: `cargo test -p jeryu -- autonomy::verdict_store`
//! Invariants:
//!   - `save()` is idempotent on `verdict.id` (`INSERT OR IGNORE`).
//!   - Before inserting a new verdict for an existing (repo, merge_request)
//!     pair, every prior non-superseded row for that pair is marked
//!     `superseded_at = save_at`. This keeps `load_latest` cheap and gives
//!     `list_active` a single boolean to filter on.
//!   - `body_json` is the source of truth: the full `VibeGateVerdict` is
//!     re-serialized losslessly. Per-column fields exist only for indexed
//!     queries (tip1 Law 4: exact-SHA binding; tip9: re-evaluable verdicts).
//!   - This store does NOT enforce signing — that's the launch_ledger's job.
//!     The daemon may persist unsigned verdicts here for replay/debug.
//!
//! Wave 11.A: the SQL queries moved to `src/db/autonomy_repo.rs`.
//! `SqlVerdictStore` is a thin wrapper that forwards trait calls to
//! `AutonomyRepo`. Public API is unchanged.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::db::AnyPool;
use crate::db::autonomy_repo::AutonomyRepo;

use super::types::VibeGateVerdict;

#[async_trait]
pub trait VerdictStore: Send + Sync {
    /// Persist a verdict. Idempotent on `verdict.id`. Marks prior
    /// non-superseded rows for the same (repo, merge_request) pair as
    /// superseded before the new row is inserted.
    async fn save(&self, verdict: &VibeGateVerdict) -> Result<()>;

    /// Return the most-recent non-superseded verdict for a (repo,
    /// merge_request) pair. `merge_request = None` matches rows with a
    /// NULL `merge_request` column.
    async fn load_latest(
        &self,
        repo: &str,
        merge_request: Option<&str>,
    ) -> Result<Option<VibeGateVerdict>>;

    /// Return all currently-active verdicts: not superseded, not expired,
    /// and not in the terminal `reject` decision. Ordered by `created_at`
    /// ascending so a daemon can poll oldest-first.
    async fn list_active(&self, now: DateTime<Utc>) -> Result<Vec<VibeGateVerdict>>;

    /// Mark one specific verdict row as superseded. No-op if already
    /// superseded.
    async fn supersede(&self, verdict_id: &str, now: DateTime<Utc>) -> Result<()>;
}

/// SQL-backed `VerdictStore`. Mirrors the style of `SqlLedger` (Wave 1.2).
#[derive(Debug, Clone)]
pub struct SqlVerdictStore {
    repo: AutonomyRepo,
}

impl SqlVerdictStore {
    pub fn new(pool: AnyPool) -> Self {
        Self {
            repo: AutonomyRepo::new(pool),
        }
    }
}

#[async_trait]
impl VerdictStore for SqlVerdictStore {
    async fn save(&self, verdict: &VibeGateVerdict) -> Result<()> {
        self.repo.verdict_save(verdict).await
    }

    async fn load_latest(
        &self,
        repo: &str,
        merge_request: Option<&str>,
    ) -> Result<Option<VibeGateVerdict>> {
        self.repo.verdict_load_latest(repo, merge_request).await
    }

    async fn list_active(&self, now: DateTime<Utc>) -> Result<Vec<VibeGateVerdict>> {
        self.repo.verdict_list_active(now).await
    }

    async fn supersede(&self, verdict_id: &str, now: DateTime<Utc>) -> Result<()> {
        self.repo.verdict_supersede(verdict_id, now).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::{
        GateDecision, RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
    };
    use crate::db::autonomy_repo::fresh_autonomy_pool;
    use chrono::Duration;

    async fn fresh_db() -> AnyPool {
        // Test fixture moved to the db boundary so this file no longer
        // imports `sqlx::` (closes HLT-006).
        fresh_autonomy_pool().await
    }

    /// Mint a verdict with a clean id (timestamp + head_sha tail) so test
    /// collisions are obvious. `mr` of `None` means "no merge_request".
    fn mint_verdict(
        repo: &str,
        mr: Option<&str>,
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
            merge_request: mr.map(|s| s.to_string()),
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
        let store = SqlVerdictStore::new(fresh_db().await);
        let now = Utc::now();
        let v = mint_verdict(
            "owner/repo",
            Some("!42"),
            "abc12345",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        store.save(&v).await.unwrap();
        let got = store
            .load_latest("owner/repo", Some("!42"))
            .await
            .unwrap()
            .expect("verdict should round-trip");
        assert_eq!(got.id, v.id);
        assert_eq!(got.repo, v.repo);
        assert_eq!(got.merge_request, v.merge_request);
        assert_eq!(got.decision, GateDecision::AllowMerge);
    }

    #[tokio::test]
    async fn save_is_idempotent_on_id() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let now = Utc::now();
        let v = mint_verdict(
            "owner/repo",
            Some("!1"),
            "ffff0001",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(30),
        );
        store.save(&v).await.unwrap();
        store.save(&v).await.unwrap();
        store.save(&v).await.unwrap();
        let active = store.list_active(now).await.unwrap();
        assert_eq!(active.len(), 1, "same id must not insert twice");
        assert_eq!(active[0].id, v.id);
    }

    #[tokio::test]
    async fn save_supersedes_prior_verdicts_for_same_repo_and_mr() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let t0 = Utc::now();
        let v1 = mint_verdict(
            "owner/repo",
            Some("!9"),
            "aaaa1111",
            GateDecision::AllowMerge,
            t0,
            t0 + Duration::minutes(60),
        );
        let v2 = mint_verdict(
            "owner/repo",
            Some("!9"),
            "bbbb2222",
            GateDecision::AllowMerge,
            t0 + Duration::seconds(5),
            t0 + Duration::minutes(60),
        );
        store.save(&v1).await.unwrap();
        store.save(&v2).await.unwrap();
        // load_latest must return v2.
        let got = store
            .load_latest("owner/repo", Some("!9"))
            .await
            .unwrap()
            .expect("latest must exist");
        assert_eq!(got.id, v2.id, "newer save must win");
        // list_active must include only v2 — v1 was superseded.
        let active = store.list_active(t0).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, v2.id);
    }

    #[tokio::test]
    async fn load_latest_returns_none_for_unknown_pair() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let now = Utc::now();
        let v = mint_verdict(
            "owner/repo",
            Some("!1"),
            "11112222",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        store.save(&v).await.unwrap();
        assert!(
            store
                .load_latest("owner/other", Some("!1"))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .load_latest("owner/repo", Some("!999"))
                .await
                .unwrap()
                .is_none()
        );
        // None vs Some("!1") are distinct: a NULL-mr lookup must not match.
        assert!(
            store
                .load_latest("owner/repo", None)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn load_latest_returns_most_recent_when_multiple() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let t0 = Utc::now();
        // Three saves to the same (repo, mr) at strictly increasing times.
        for i in 0..3 {
            let v = mint_verdict(
                "owner/repo",
                Some("!7"),
                &format!("dead000{i}"),
                GateDecision::AllowMerge,
                t0 + Duration::seconds(i as i64),
                t0 + Duration::minutes(60),
            );
            store.save(&v).await.unwrap();
        }
        let got = store
            .load_latest("owner/repo", Some("!7"))
            .await
            .unwrap()
            .expect("latest must exist");
        // The last save's head_sha tail is "dead0002".
        assert!(
            got.head_sha.ends_with("dead0002"),
            "got head_sha={}",
            got.head_sha
        );
    }

    #[tokio::test]
    async fn list_active_excludes_expired_verdicts() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let now = Utc::now();
        let expired = mint_verdict(
            "owner/repo",
            Some("!1"),
            "11110000",
            GateDecision::AllowMerge,
            now - Duration::minutes(120),
            now - Duration::minutes(60), // already expired
        );
        let live = mint_verdict(
            "owner/repo",
            Some("!2"),
            "22220000",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        store.save(&expired).await.unwrap();
        store.save(&live).await.unwrap();
        let active = store.list_active(now).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, live.id);
    }

    #[tokio::test]
    async fn list_active_excludes_rejected_verdicts() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let now = Utc::now();
        let allow = mint_verdict(
            "owner/repo",
            Some("!a"),
            "aaaa0000",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        let reject = mint_verdict(
            "owner/repo",
            Some("!r"),
            "ffff0000",
            GateDecision::Reject,
            now,
            now + Duration::minutes(60),
        );
        let human = mint_verdict(
            "owner/repo",
            Some("!h"),
            "cccc0000",
            GateDecision::RequireHuman,
            now,
            now + Duration::minutes(60),
        );
        store.save(&allow).await.unwrap();
        store.save(&reject).await.unwrap();
        store.save(&human).await.unwrap();
        let active = store.list_active(now).await.unwrap();
        let ids: Vec<&str> = active.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(
            active.len(),
            2,
            "reject must be excluded; got ids={:?}",
            ids
        );
        assert!(ids.contains(&allow.id.as_str()));
        assert!(ids.contains(&human.id.as_str()));
        assert!(!ids.contains(&reject.id.as_str()));
    }

    #[tokio::test]
    async fn list_active_excludes_superseded_verdicts() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let t0 = Utc::now();
        let v1 = mint_verdict(
            "owner/repo",
            Some("!1"),
            "11110001",
            GateDecision::AllowMerge,
            t0,
            t0 + Duration::minutes(60),
        );
        let v2 = mint_verdict(
            "owner/repo",
            Some("!1"),
            "11110002",
            GateDecision::AllowMerge,
            t0 + Duration::seconds(1),
            t0 + Duration::minutes(60),
        );
        store.save(&v1).await.unwrap();
        store.save(&v2).await.unwrap();
        let active = store.list_active(t0).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, v2.id);
    }

    #[tokio::test]
    async fn list_active_orders_by_created_at_ascending() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let t0 = Utc::now();
        // Save with different mr so none supersede each other.
        let v_b = mint_verdict(
            "owner/repo",
            Some("!b"),
            "bbbb0001",
            GateDecision::AllowMerge,
            t0 + Duration::seconds(20),
            t0 + Duration::minutes(60),
        );
        let v_a = mint_verdict(
            "owner/repo",
            Some("!a"),
            "aaaa0001",
            GateDecision::AllowMerge,
            t0 + Duration::seconds(10),
            t0 + Duration::minutes(60),
        );
        let v_c = mint_verdict(
            "owner/repo",
            Some("!c"),
            "cccc0001",
            GateDecision::AllowMerge,
            t0 + Duration::seconds(30),
            t0 + Duration::minutes(60),
        );
        // Save out of order; list_active must still order by created_at ASC.
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
        let store = SqlVerdictStore::new(fresh_db().await);
        let now = Utc::now();
        let v = mint_verdict(
            "owner/repo",
            Some("!1"),
            "fade0001",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        store.save(&v).await.unwrap();
        assert_eq!(store.list_active(now).await.unwrap().len(), 1);

        store
            .supersede(&v.id, now + Duration::seconds(5))
            .await
            .unwrap();
        assert_eq!(
            store.list_active(now).await.unwrap().len(),
            0,
            "superseded row must drop out of list_active"
        );

        // Second supersede on the same id must be a no-op (not an error).
        store
            .supersede(&v.id, now + Duration::seconds(10))
            .await
            .expect("idempotent supersede");

        // Supersede of an unknown id is also a no-op.
        store
            .supersede("vgv_nope", now)
            .await
            .expect("unknown id is a no-op");
    }

    #[tokio::test]
    async fn body_json_is_source_of_truth_after_round_trip() {
        use crate::autonomy::types::{ReviewDecision, ReviewerRole};
        let store = SqlVerdictStore::new(fresh_db().await);
        let now = Utc::now();
        let mut v = mint_verdict(
            "owner/repo",
            Some("!42"),
            "beef0001",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        // Decorate with fields that ONLY survive via body_json (no dedicated
        // column on the verdicts table): approval_receipts, hard_stops, etc.
        v.hard_stops = vec!["security:high".into(), "tests:full_required".into()];
        v.approval_receipts = vec![
            VerdictReceiptRef {
                role: ReviewerRole::Security,
                agent_id: "reviewer-security.v1".into(),
                receipt_digest: "sha256:cafe".into(),
                decision: ReviewDecision::Pass,
                not_author: true,
            },
            VerdictReceiptRef {
                role: ReviewerRole::Judge,
                agent_id: "judge.v1".into(),
                receipt_digest: "sha256:beef".into(),
                decision: ReviewDecision::Pass,
                not_author: true,
            },
        ];
        store.save(&v).await.unwrap();
        let got = store
            .load_latest("owner/repo", Some("!42"))
            .await
            .unwrap()
            .expect("verdict must load");
        assert_eq!(got, v, "body_json must round-trip every field losslessly");
    }

    #[tokio::test]
    async fn concurrent_save_no_corruption_with_four_tasks() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let t0 = Utc::now();
        let mut handles = Vec::new();
        for task in 0..4 {
            let store = store.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..5 {
                    // Each task uses its own mr so saves don't supersede
                    // each other and we land 20 active rows total.
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
        assert_eq!(
            active.len(),
            20,
            "4 tasks * 5 distinct verdicts must produce 20 active rows"
        );
        // load_latest for one of the (repo, mr) pairs must return that pair's verdict.
        let got = store
            .load_latest("owner/repo", Some("!t2-3"))
            .await
            .unwrap()
            .expect("pair must exist");
        assert_eq!(got.repo, "owner/repo");
        assert_eq!(got.merge_request.as_deref(), Some("!t2-3"));
    }

    #[tokio::test]
    async fn save_with_unsigned_verdict_succeeds_for_replay_use_case() {
        let store = SqlVerdictStore::new(fresh_db().await);
        let now = Utc::now();
        let mut v = mint_verdict(
            "owner/repo",
            Some("!unsigned"),
            "0bad0001",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        // Explicitly stub signature: this store does NOT enforce signing,
        // unlike `SqlLedger::append`. The daemon uses this path for replay
        // and shadow-mode runs where signing is intentionally absent.
        v.signature = Signature::stub();
        store
            .save(&v)
            .await
            .expect("verdict_store accepts unsigned verdicts (replay use case)");
        let got = store
            .load_latest("owner/repo", Some("!unsigned"))
            .await
            .unwrap()
            .expect("unsigned verdict must round-trip");
        assert_eq!(got.signature.algo, "stub");
    }
}
