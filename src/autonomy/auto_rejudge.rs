//! Owner: Evidence Gate / autonomous-delivery daemon (Wave 8.C)
//! Proof: `cargo test -p jeryu --lib autonomy::auto_rejudge`
//! Invariants:
//!   - One rejudge run = one fresh `EvidencePack` build, one orchestrated
//!     reviewer pass, one pure `judge()` fusion, one signed
//!     `VerdictIssued` ledger entry, and one verdict save+supersede pair.
//!   - The new verdict supersedes the old one for the same
//!     `(repo, merge_request)` pair — this is enforced by the
//!     `VerdictStore::save()` contract.
//!   - Orchestrator failures degrade to "no receipts" rather than aborting:
//!     a missing reviewer is itself signal for `judge()` (insufficient
//!     quorum → `RequireHuman`), not a structural error to bubble up.
//!   - Pack-builder failures DO bubble up: without an EvidencePack there
//!     is nothing to judge against (Tip1 Law 4 demands SHA-bound evidence).
//!
//! Cross-wave imports: the `EvidencePackBuilder` trait lives in
//! `src/autonomy/evidence_pack_builder.rs` (Wave 8.A) and the
//! `ReviewerOrchestrator` trait lives in `src/agent_review/orchestrator.rs`
//! (Wave 8.B). Both signatures are pinned to the contracts the Wave 8.C
//! handoff specified, so this file is just composition.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::Serialize;
use serde_json::json;

use crate::agent_review::judge::{JudgeInputs, judge};
use crate::agent_review::orchestrator::ReviewerOrchestrator;
use crate::autonomy::evidence_pack_builder::EvidencePackBuilder;
use crate::autonomy::ledger::{SqlLedger, sign_entry};
use crate::autonomy::policy_yaml::PolicyBundle;
use crate::autonomy::signing::{EdSigningKey, Signature};
use crate::autonomy::types::{
    AgentApprovalReceipt, GateDecision, LaunchLedgerEntry, LedgerKind, ReviewerRole, SchemaTag,
    VibeGateVerdict,
};
use crate::autonomy::verdict_store::VerdictStore;
use crate::git_host::RepoRef;

// ---------------------------------------------------------------------------
// Public service surface
// ---------------------------------------------------------------------------

/// Composes pack-builder + orchestrator + judge + verdict-store into a
/// single in-process unit. One call to [`AutoRejudgeService::rejudge`]
/// turns Wave 7's "detect-only" daemon into a self-correcting daemon.
pub struct AutoRejudgeService {
    pub pack_builder: Arc<dyn EvidencePackBuilder>,
    pub orchestrator: Arc<dyn ReviewerOrchestrator>,
    pub verdict_store: Arc<dyn VerdictStore>,
    pub ledger: SqlLedger,
    pub signing_key: Arc<EdSigningKey>,
    pub policy: Arc<PolicyBundle>,
}

/// Structured outcome of one rejudge run. The daemon folds these into
/// the per-tick `RejudgeRecord` it already produces.
#[derive(Debug, Clone, Serialize)]
pub struct RejudgeOutcome {
    pub repo: String,
    pub mr_iid: String,
    pub old_verdict_id: String,
    pub new_verdict_id: String,
    pub new_decision: GateDecision,
    pub hard_stops: Vec<String>,
    pub receipts_count: usize,
    pub elapsed_ms: u64,
}

impl AutoRejudgeService {
    pub fn new(
        pack_builder: Arc<dyn EvidencePackBuilder>,
        orchestrator: Arc<dyn ReviewerOrchestrator>,
        verdict_store: Arc<dyn VerdictStore>,
        ledger: SqlLedger,
        signing_key: Arc<EdSigningKey>,
        policy: Arc<PolicyBundle>,
    ) -> Self {
        Self {
            pack_builder,
            orchestrator,
            verdict_store,
            ledger,
            signing_key,
            policy,
        }
    }

    /// Run one full rejudge cycle for a single PR. See module docs for the
    /// step-by-step algorithm.
    pub async fn rejudge(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
        old_verdict: &VibeGateVerdict,
    ) -> Result<RejudgeOutcome> {
        let started = Instant::now();

        // 1. Fresh signed evidence pack from the host's current view of the PR.
        //    A failure here is structural — we cannot judge without bound
        //    evidence (Tip1 Law 4) — so we bubble it up.
        let pack = self.pack_builder.build(repo, mr_iid).await?;

        // 2. Look up required reviewer roles for the pack's risk tier.
        // A tier with no configured quorum (e.g. R0) returns `None` from
        // `quorum_for`; an empty role list is the documented semantic — it
        // tells `judge()` "no reviewers required" rather than signaling an
        // error path.
        let required_roles: Vec<ReviewerRole> = self
            .policy
            .quorum_for(pack.risk)
            .map(|q| q.roles.clone())
            .unwrap_or_default();

        // 3. Run the orchestrator. An orchestrator-level Err degrades to
        //    "no receipts" — `judge()` will treat that as insufficient
        //    quorum and emit `RequireHuman`. The point of the daemon is to
        //    keep producing signed verdicts; a transient reviewer outage
        //    must not abort the gate.
        let receipts: Vec<AgentApprovalReceipt> =
            match self.orchestrator.run_all(&pack, &required_roles, "").await {
                Ok(rs) => rs,
                Err(_) => Vec::new(),
            };

        // 4. Pure policy fusion. No side effects in `judge()`.
        let outcome = judge(JudgeInputs {
            pack: &pack,
            receipts: &receipts,
            policy: &self.policy,
            repo: &repo.slug(),
            target_branch: &old_verdict.target_branch,
            merge_request: Some(mr_iid),
            author_agent: None,
            external_hard_stops: &[],
        });
        let new_verdict = outcome.verdict;

        // 5. Persist the new verdict. `save()` is idempotent on id AND
        //    supersedes any prior non-superseded verdict for the same
        //    (repo, mr) pair — including `old_verdict`.
        self.verdict_store.save(&new_verdict).await?;

        // 6. Sign + append a `VerdictIssued` ledger entry stamped with
        //    `wave_scope = "auto_rejudge"` so replay tooling can tell this
        //    apart from a normal first-issue verdict.
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let mut entry =
            build_auto_rejudge_entry(&new_verdict, &old_verdict.id, elapsed_ms, &self.signing_key);
        sign_entry(&mut entry, &self.signing_key);
        self.ledger.append(&entry).await?;

        Ok(RejudgeOutcome {
            repo: repo.slug(),
            mr_iid: mr_iid.to_string(),
            old_verdict_id: old_verdict.id.clone(),
            new_verdict_id: new_verdict.id.clone(),
            new_decision: new_verdict.decision,
            hard_stops: new_verdict.hard_stops.clone(),
            receipts_count: receipts.len(),
            elapsed_ms,
        })
    }
}

/// Build an unsigned `VerdictIssued` ledger entry that carries the
/// `auto_rejudge` wave-scope marker and the link back to the superseded
/// verdict id. Caller signs immediately before append.
fn build_auto_rejudge_entry(
    new_verdict: &VibeGateVerdict,
    old_verdict_id: &str,
    elapsed_ms: u64,
    signing_key: &EdSigningKey,
) -> LaunchLedgerEntry {
    // Choose the ledger kind exactly the way `verdict_issued_entry` does so
    // downstream filters keep working. (RequireHuman → HumanEscalationRequested.)
    let kind = match new_verdict.decision {
        GateDecision::AllowMerge => LedgerKind::VerdictIssued,
        GateDecision::Reject => LedgerKind::VerdictIssued,
        GateDecision::RequireHuman => LedgerKind::HumanEscalationRequested,
    };
    let payload = json!({
        "wave_scope": "auto_rejudge",
        "old_verdict_id": old_verdict_id,
        "new_verdict_id": new_verdict.id,
        "new_decision": new_verdict.decision,
        "elapsed_ms": elapsed_ms,
    });
    LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: format!("ll_auto_rejudge_{}", new_verdict.id),
        kind,
        subject_id: new_verdict.id.clone(),
        repo: Some(new_verdict.repo.clone()),
        payload,
        recorded_at: new_verdict.created_at,
        actor: format!("auto-rejudge-service ({})", signing_key.key_id),
        // Unsigned marker — `sign_entry()` overwrites with a real ed25519
        // signature immediately before `SqlLedger::append()`, which refuses
        // any non-ed25519 algo at the boundary.
        signature: Signature::default_unsigned(),
    }
}

// ---------------------------------------------------------------------------
// Test helpers — `pub(crate)` so daemon tests can reuse them.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use crate::autonomy::evidence::{EvidenceInputs, build_evidence_pack};
    use crate::autonomy::types::{
        EvidencePack, ReviewDecision, RiskTier, RollbackSection, RollbackStrategy, ScanOutcome,
        SecuritySection, SupplyChainSection, TestsSection,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;

    /// A pack builder you can pre-seed with either a canned pack OR an
    /// error. Convenient for the rejudge service tests that don't want to
    /// stand up a full FakeGitHost just to construct a single EvidencePack.
    pub struct FakeEvidencePackBuilder {
        pack: Mutex<Option<EvidencePack>>,
        fail: Mutex<Option<String>>,
        /// Records the (repo_slug, mr_iid) tuples passed to `build`.
        pub calls: Mutex<Vec<(String, String)>>,
    }

    impl FakeEvidencePackBuilder {
        pub fn with_pack(pack: EvidencePack) -> Arc<Self> {
            Arc::new(Self {
                pack: Mutex::new(Some(pack)),
                fail: Mutex::new(None),
                calls: Mutex::new(Vec::new()),
            })
        }

        pub fn with_error(msg: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                pack: Mutex::new(None),
                fail: Mutex::new(Some(msg.into())),
                calls: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl EvidencePackBuilder for FakeEvidencePackBuilder {
        async fn build(&self, repo: &RepoRef, mr_iid: &str) -> Result<EvidencePack> {
            self.calls
                .lock()
                .unwrap()
                .push((repo.slug(), mr_iid.to_string()));
            if let Some(msg) = self.fail.lock().unwrap().clone() {
                return Err(anyhow::anyhow!(msg));
            }
            let p = self
                .pack
                .lock()
                .unwrap()
                .clone()
                .expect("FakeEvidencePackBuilder must be seeded with a pack");
            Ok(p)
        }
    }

    /// A reviewer orchestrator that returns a canned set of receipts (already
    /// SHA-bound to the canned pack), or an error. Distinct from the Wave 8.B
    /// `FakeReviewerOrchestrator` only in that we can construct it inline
    /// without that module yet existing.
    pub struct CannedOrchestrator {
        receipts: Mutex<Vec<AgentApprovalReceipt>>,
        fail: Mutex<Option<String>>,
        pub calls: Mutex<Vec<Vec<ReviewerRole>>>,
    }

    impl CannedOrchestrator {
        pub fn with_receipts(receipts: Vec<AgentApprovalReceipt>) -> Arc<Self> {
            Arc::new(Self {
                receipts: Mutex::new(receipts),
                fail: Mutex::new(None),
                calls: Mutex::new(Vec::new()),
            })
        }

        pub fn with_error(msg: impl Into<String>) -> Arc<Self> {
            Arc::new(Self {
                receipts: Mutex::new(Vec::new()),
                fail: Mutex::new(Some(msg.into())),
                calls: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl ReviewerOrchestrator for CannedOrchestrator {
        async fn run_all(
            &self,
            _pack: &EvidencePack,
            required_roles: &[ReviewerRole],
            _diff_text: &str,
        ) -> Result<Vec<AgentApprovalReceipt>> {
            self.calls.lock().unwrap().push(required_roles.to_vec());
            if let Some(msg) = self.fail.lock().unwrap().clone() {
                return Err(anyhow::anyhow!(msg));
            }
            Ok(self.receipts.lock().unwrap().clone())
        }
    }

    /// Build a deterministic EvidencePack at the requested tier. Uses
    /// constant SHAs so receipts can be SHA-bound by reconstructing the
    /// same head/policy. The pack is stamped with a plausible `ed25519`
    /// signature so the `evidence_signature_invalid` hard stop does not
    /// fire in tests (we're testing the rejudge composer, not the
    /// signature-verification surface).
    pub fn canned_pack(tier: RiskTier) -> EvidencePack {
        let mut p = build_evidence_pack(EvidenceInputs {
            repo: "owner/repo",
            source_branch: "agent/x",
            target_branch: "main",
            head_sha: "aa11".repeat(10).leak(),
            base_sha: "bb22".repeat(10).leak(),
            policy_sha: "cc33".repeat(10).leak(),
            author_agent: Some("builder.x"),
            intent_id: None,
            risk: tier,
            changed_files: vec![],
            claims: vec![],
            tests: TestsSection {
                targeted: vec![],
                full_required: false,
                skipped: vec![],
                coverage_delta: None,
            },
            security: SecuritySection {
                sast: ScanOutcome::Passed,
                dependency_scan: ScanOutcome::Passed,
                secret_scan: ScanOutcome::Passed,
            },
            supply_chain: SupplyChainSection::default(),
            rollback: RollbackSection {
                strategy: RollbackStrategy::RevertCommit,
                feature_flag: None,
                data_migration_reversible: Some(true),
            },
            legacy_receipts: vec![],
        });
        p.signature = Some(Signature {
            key_id: "evidence-builder.test".into(),
            algo: "ed25519".into(),
            value: "0".repeat(128),
        });
        p
    }

    /// Mint a receipt SHA-bound to the given pack.
    pub fn bound_receipt(
        pack: &EvidencePack,
        role: ReviewerRole,
        agent: &str,
        decision: ReviewDecision,
    ) -> AgentApprovalReceipt {
        AgentApprovalReceipt {
            schema: SchemaTag::new(),
            id: format!("aar_{agent}_{:?}", role),
            evidence_pack_id: pack.id.clone(),
            role,
            agent_id: agent.into(),
            prompt_sha: None,
            provider: None,
            model: None,
            temperature: None,
            seed: None,
            raw_response_sha: Some(format!("sha256:{agent}")),
            head_sha: pack.head_sha.clone(),
            policy_sha: pack.policy_sha.clone(),
            decision,
            reason: None,
            findings: vec![],
            not_author: true,
            tokens: Default::default(),
            created_at: Utc::now(),
            signature: Signature::stub(),
        }
    }

    /// Mint a synthetic prior `VibeGateVerdict`. Test-only — production
    /// verdicts come from `judge()`.
    pub fn mint_old_verdict(repo: &str, mr: &str) -> VibeGateVerdict {
        use crate::autonomy::types::{RiskTier, VerdictReceiptRef};
        let now = Utc::now();
        VibeGateVerdict {
            schema: SchemaTag::new(),
            id: format!("vgv_old_{}", mr.replace('!', "")),
            evidence_pack_id: "ep_old".into(),
            merge_request: Some(mr.into()),
            repo: repo.into(),
            target_branch: "main".into(),
            head_sha: "old-head".repeat(5),
            policy_sha: "old-policy".repeat(4),
            evidence_pack_digest: "sha256:old".into(),
            risk: RiskTier::R2,
            hard_stops: vec![],
            required_reviews: vec![],
            approval_receipts: Vec::<VerdictReceiptRef>::new(),
            decision: GateDecision::AllowMerge,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at: now + chrono::Duration::minutes(60),
            created_at: now,
            signature: Signature::stub(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;
    use crate::autonomy::ledger::LedgerFilter;
    use crate::autonomy::types::{ReviewDecision, ReviewerRole, RiskTier};
    use crate::autonomy::verdict_store::SqlVerdictStore;
    use crate::db::AnyPool;
    use crate::db::autonomy_repo::fresh_autonomy_pool;
    use std::path::Path;

    // -- DB harness ----------------------------------------------------------

    async fn fresh_db() -> AnyPool {
        // Test fixture moved to the db boundary so this file no longer
        // imports `sqlx::` (closes HLT-006).
        fresh_autonomy_pool().await
    }

    fn policy() -> Arc<PolicyBundle> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".autonomy/policies");
        Arc::new(PolicyBundle::from_dir(&dir).expect("policy bundle loads"))
    }

    fn signing_key() -> Arc<EdSigningKey> {
        Arc::new(EdSigningKey::generate("auto-rejudge.test"))
    }

    struct Harness {
        service: AutoRejudgeService,
        verdict_store: Arc<SqlVerdictStore>,
        ledger: SqlLedger,
        repo: RepoRef,
    }

    impl Harness {
        async fn new(
            pack_builder: Arc<dyn EvidencePackBuilder>,
            orchestrator: Arc<dyn ReviewerOrchestrator>,
        ) -> Self {
            let pool = fresh_db().await;
            let verdict_store = Arc::new(SqlVerdictStore::new(pool.clone()));
            let ledger = SqlLedger::new(pool.clone());
            let service = AutoRejudgeService::new(
                pack_builder,
                orchestrator,
                verdict_store.clone() as Arc<dyn VerdictStore>,
                ledger.clone(),
                signing_key(),
                policy(),
            );
            Self {
                service,
                verdict_store,
                ledger,
                repo: RepoRef::parse("owner/repo").unwrap(),
            }
        }
    }

    // -- Tests --------------------------------------------------------------

    #[tokio::test]
    async fn rejudge_with_clean_diff_returns_allow_merge_outcome() {
        // R2: needs test_integrity + security passes for AllowMerge.
        let pack = canned_pack(RiskTier::R2);
        let receipts = vec![
            bound_receipt(
                &pack,
                ReviewerRole::TestIntegrity,
                "tester.v1",
                ReviewDecision::Pass,
            ),
            bound_receipt(
                &pack,
                ReviewerRole::Security,
                "sec.v1",
                ReviewDecision::Pass,
            ),
        ];
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(receipts);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!42");

        let outcome = h
            .service
            .rejudge(&h.repo, "!42", &old)
            .await
            .expect("rejudge OK");

        assert_eq!(outcome.repo, "owner/repo");
        assert_eq!(outcome.mr_iid, "!42");
        assert_eq!(outcome.old_verdict_id, old.id);
        assert_eq!(outcome.new_decision, GateDecision::AllowMerge);
        assert!(outcome.hard_stops.is_empty(), "no hard stops on clean diff");
        assert_eq!(outcome.receipts_count, 2);
    }

    #[tokio::test]
    async fn rejudge_with_blocking_reviewer_returns_reject_outcome() {
        // Any reviewer Block → `reviewer_blocked` hard stop → Reject.
        let pack = canned_pack(RiskTier::R2);
        let receipts = vec![
            bound_receipt(
                &pack,
                ReviewerRole::TestIntegrity,
                "tester.v1",
                ReviewDecision::Pass,
            ),
            bound_receipt(
                &pack,
                ReviewerRole::Security,
                "sec.v1",
                ReviewDecision::Block,
            ),
        ];
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(receipts);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!7");

        let outcome = h.service.rejudge(&h.repo, "!7", &old).await.unwrap();

        assert_eq!(outcome.new_decision, GateDecision::Reject);
        assert!(
            outcome.hard_stops.iter().any(|s| s == "reviewer_blocked"),
            "expected reviewer_blocked hard stop; got {:?}",
            outcome.hard_stops
        );
    }

    #[tokio::test]
    async fn rejudge_supersedes_old_verdict_in_store() {
        // Save the old verdict first; after rejudge, load_latest must
        // return the NEW one (the old one is superseded).
        let pack = canned_pack(RiskTier::R2);
        let receipts = vec![
            bound_receipt(
                &pack,
                ReviewerRole::TestIntegrity,
                "t",
                ReviewDecision::Pass,
            ),
            bound_receipt(&pack, ReviewerRole::Security, "s", ReviewDecision::Pass),
        ];
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(receipts);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!99");
        h.verdict_store.save(&old).await.unwrap();
        assert_eq!(
            h.verdict_store
                .load_latest("owner/repo", Some("!99"))
                .await
                .unwrap()
                .map(|v| v.id),
            Some(old.id.clone()),
            "old verdict must be loadable before rejudge"
        );

        let outcome = h.service.rejudge(&h.repo, "!99", &old).await.unwrap();

        let after = h
            .verdict_store
            .load_latest("owner/repo", Some("!99"))
            .await
            .unwrap()
            .expect("new verdict must be loadable");
        assert_eq!(after.id, outcome.new_verdict_id);
        assert_ne!(after.id, old.id, "new verdict id must differ from old");
    }

    #[tokio::test]
    async fn rejudge_saves_new_verdict_to_store() {
        // Without seeding the old verdict — the service should still
        // produce + save a brand-new one.
        let pack = canned_pack(RiskTier::R2);
        let receipts = vec![
            bound_receipt(
                &pack,
                ReviewerRole::TestIntegrity,
                "t",
                ReviewDecision::Pass,
            ),
            bound_receipt(&pack, ReviewerRole::Security, "s", ReviewDecision::Pass),
        ];
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(receipts);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!11");

        let outcome = h.service.rejudge(&h.repo, "!11", &old).await.unwrap();

        let stored = h
            .verdict_store
            .load_latest("owner/repo", Some("!11"))
            .await
            .unwrap()
            .expect("stored verdict");
        assert_eq!(stored.id, outcome.new_verdict_id);
        assert_eq!(stored.decision, outcome.new_decision);
    }

    #[tokio::test]
    async fn rejudge_appends_ledger_entry_with_auto_rejudge_scope() {
        let pack = canned_pack(RiskTier::R2);
        let receipts = vec![
            bound_receipt(
                &pack,
                ReviewerRole::TestIntegrity,
                "t",
                ReviewDecision::Pass,
            ),
            bound_receipt(&pack, ReviewerRole::Security, "s", ReviewDecision::Pass),
        ];
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(receipts);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!1");

        let outcome = h.service.rejudge(&h.repo, "!1", &old).await.unwrap();

        let entries = h.ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(entries.len(), 1, "exactly one ledger entry per rejudge");
        let e = &entries[0];
        assert_eq!(e.subject_id, outcome.new_verdict_id);
        assert_eq!(e.repo.as_deref(), Some("owner/repo"));
        assert_eq!(e.payload["wave_scope"], "auto_rejudge");
        assert_eq!(e.payload["old_verdict_id"], old.id);
        // SqlLedger refuses stub/hmac, so a successful append proves ed25519.
        assert_eq!(e.signature.algo, "ed25519");
    }

    #[tokio::test]
    async fn rejudge_elapsed_ms_is_populated() {
        let pack = canned_pack(RiskTier::R2);
        let receipts = vec![
            bound_receipt(
                &pack,
                ReviewerRole::TestIntegrity,
                "t",
                ReviewDecision::Pass,
            ),
            bound_receipt(&pack, ReviewerRole::Security, "s", ReviewDecision::Pass),
        ];
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(receipts);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!1");

        let outcome = h.service.rejudge(&h.repo, "!1", &old).await.unwrap();

        // elapsed_ms is u64; field is just always present. The structural
        // assertion that matters: it round-trips through the ledger payload.
        let entries = h.ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(entries.len(), 1);
        let payload_elapsed = entries[0].payload["elapsed_ms"].as_u64().unwrap();
        assert_eq!(payload_elapsed, outcome.elapsed_ms);
    }

    #[tokio::test]
    async fn rejudge_returns_err_when_pack_builder_fails() {
        // Pack-builder failure is structural (Tip1 Law 4: no evidence → no
        // verdict). The service must surface the error so the daemon can
        // record it as a TickError rather than silently skipping.
        let pb = FakeEvidencePackBuilder::with_error("simulated git host outage");
        let orch = CannedOrchestrator::with_receipts(vec![]);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!1");

        let err = h.service.rejudge(&h.repo, "!1", &old).await.unwrap_err();

        assert!(
            err.to_string().contains("simulated git host outage"),
            "expected pack-builder error to surface; got {err}"
        );
        // Nothing got saved or appended.
        assert!(
            h.verdict_store
                .load_latest("owner/repo", Some("!1"))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            h.ledger
                .list(&LedgerFilter::default())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn rejudge_orchestrator_error_surfaces_as_judge_with_no_receipts() {
        // Orchestrator-level Err → treat as "no receipts" rather than
        // aborting. judge() will then escalate to RequireHuman because
        // R2 needs 2 reviewers and we have 0.
        let pack = canned_pack(RiskTier::R2);
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_error("reviewer model timeout");
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!5");

        let outcome = h
            .service
            .rejudge(&h.repo, "!5", &old)
            .await
            .expect("orchestrator Err must not abort the service");

        assert_eq!(outcome.receipts_count, 0, "no receipts collected");
        // R2 with 0 reviewers → quorum insufficient → RequireHuman.
        assert_eq!(outcome.new_decision, GateDecision::RequireHuman);
    }

    #[tokio::test]
    async fn rejudge_with_empty_required_roles_still_judges_and_writes_verdict() {
        // R0 has no required roles. judge() must still emit a verdict and
        // the service must still save+sign it. Use R0 (no quorum + no
        // human_required) so we get AllowMerge with zero receipts.
        let pack = canned_pack(RiskTier::R0);
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(vec![]);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!r0");

        let outcome = h.service.rejudge(&h.repo, "!r0", &old).await.unwrap();

        assert_eq!(outcome.new_decision, GateDecision::AllowMerge);
        assert_eq!(outcome.receipts_count, 0);
        // Verdict persisted + ledger appended even with no reviewers.
        assert!(
            h.verdict_store
                .load_latest("owner/repo", Some("!r0"))
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            h.ledger.list(&LedgerFilter::default()).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn rejudge_with_high_risk_tier_requires_human_decision() {
        // R4: human_required + fail_closed_without_human. No automated
        // reviewer set can satisfy this → judge() must emit RequireHuman.
        let pack = canned_pack(RiskTier::R4);
        // Even if we hand in receipts, R4 forces human. Verify the wiring.
        let receipts = vec![bound_receipt(
            &pack,
            ReviewerRole::Security,
            "sec.v1",
            ReviewDecision::Pass,
        )];
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(receipts);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!r4");

        let outcome = h.service.rejudge(&h.repo, "!r4", &old).await.unwrap();

        assert_eq!(outcome.new_decision, GateDecision::RequireHuman);
    }

    #[tokio::test]
    async fn rejudge_new_verdict_id_differs_from_old() {
        // The judge mints a fresh id every call; we just confirm the
        // service propagates it.
        let pack = canned_pack(RiskTier::R2);
        let receipts = vec![
            bound_receipt(
                &pack,
                ReviewerRole::TestIntegrity,
                "t",
                ReviewDecision::Pass,
            ),
            bound_receipt(&pack, ReviewerRole::Security, "s", ReviewDecision::Pass),
        ];
        let pb = FakeEvidencePackBuilder::with_pack(pack);
        let orch = CannedOrchestrator::with_receipts(receipts);
        let h = Harness::new(pb, orch).await;
        let old = mint_old_verdict("owner/repo", "!1");

        let outcome = h.service.rejudge(&h.repo, "!1", &old).await.unwrap();

        assert_ne!(
            outcome.new_verdict_id, outcome.old_verdict_id,
            "rejudge MUST mint a brand-new verdict id"
        );
        assert!(
            outcome.new_verdict_id.starts_with("vgv_"),
            "verdict ids start with vgv_; got {}",
            outcome.new_verdict_id
        );
    }
}
