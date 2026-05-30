//! Auto-rejudge service — composition of the Evidence-Gate pieces.
//!
//! Invariants:
//!   - One rejudge run = one fresh `EvidencePack` build, one orchestrated
//!     reviewer pass, one pure `judge()` fusion, one signed `VerdictIssued`
//!     ledger entry, and one verdict save+supersede pair.
//!   - The new verdict supersedes the old one for the same (repo, pull_request)
//!     pair (enforced by the `VerdictStore::save` contract).
//!   - Orchestrator failures degrade to "no receipts" rather than aborting: a
//!     missing reviewer is itself signal for `judge()` (insufficient quorum →
//!     `RequireHuman`), not a structural error to bubble up.
//!   - Evidence-source (pack-build) failures DO bubble up: without a SHA-bound
//!     pack there is nothing to judge against.

use crate::judge::{JudgeInputs, judge};
use crate::ledger::sign_entry;
use crate::policy_yaml::PolicyBundle;
use crate::seam::{EvidenceSource, SeamError, SeamResult, VerdictLedger, VerdictStore};
use crate::signing::{EdSigningKey, Signature};
use crate::types::{
    GateDecision, LaunchLedgerEntry, LedgerKind, ReviewerRole, SchemaTag, VibeGateVerdict,
};
use serde_json::json;
use std::sync::Arc;

/// Composes evidence-source + verdict-store + ledger + the pure judge into one
/// self-correcting unit.
pub struct AutoRejudgeService {
    pub evidence: Arc<dyn EvidenceSource>,
    pub verdict_store: Arc<dyn VerdictStore>,
    pub ledger: Arc<dyn VerdictLedger>,
    pub signing_key: Arc<EdSigningKey>,
    pub policy: Arc<PolicyBundle>,
}

/// Structured outcome of one rejudge run.
#[derive(Debug, Clone)]
pub struct RejudgeOutcome {
    pub repo: String,
    pub pr_id: String,
    pub old_verdict_id: String,
    pub new_verdict_id: String,
    pub new_decision: GateDecision,
    pub hard_stops: Vec<String>,
    pub receipts_count: usize,
}

impl AutoRejudgeService {
    pub fn new(
        evidence: Arc<dyn EvidenceSource>,
        verdict_store: Arc<dyn VerdictStore>,
        ledger: Arc<dyn VerdictLedger>,
        signing_key: Arc<EdSigningKey>,
        policy: Arc<PolicyBundle>,
    ) -> Self {
        Self {
            evidence,
            verdict_store,
            ledger,
            signing_key,
            policy,
        }
    }

    /// Run one full rejudge cycle for a single PR.
    pub async fn rejudge(
        &self,
        repo: &str,
        pr_id: &str,
        old_verdict: &VibeGateVerdict,
    ) -> SeamResult<RejudgeOutcome> {
        // 1. Fresh signed evidence pack. A failure here is structural — bubble up.
        let pack = self.evidence.build_pack(repo, pr_id).await?;

        // 2. Required reviewer roles for the pack's risk tier.
        let required_roles: Vec<ReviewerRole> = self
            .policy
            .quorum_for(pack.risk)
            .map_or_else(Vec::new, |q| q.roles.clone());

        // 3. Run the orchestrator. An Err degrades to "no receipts" — judge()
        //    treats that as insufficient quorum and emits RequireHuman.
        let receipts = self
            .evidence
            .run_reviews(&pack, &required_roles)
            .await
            .unwrap_or_default();

        // 4. Pure policy fusion. No side effects in judge().
        let outcome = judge(JudgeInputs {
            pack: &pack,
            receipts: &receipts,
            policy: &self.policy,
            repo,
            target_branch: &old_verdict.target_branch,
            pull_request: Some(pr_id),
            author_agent: None,
            external_hard_stops: &[],
        });
        let new_verdict = outcome.verdict;

        // 5. Persist the new verdict. save() is idempotent on id AND supersedes
        //    any prior non-superseded verdict for the same (repo, pr) pair.
        self.verdict_store.save(&new_verdict).await?;

        // 6. Sign + append a VerdictIssued ledger entry stamped
        //    wave_scope="auto_rejudge" so replay tooling can tell it apart.
        let mut entry = build_auto_rejudge_entry(&new_verdict, &old_verdict.id);
        sign_entry(&mut entry, &self.signing_key);
        self.ledger
            .append(&entry)
            .await
            .map_err(|e| SeamError::new("auto_rejudge", format!("append VerdictIssued: {e}")))?;

        Ok(RejudgeOutcome {
            repo: repo.to_string(),
            pr_id: pr_id.to_string(),
            old_verdict_id: old_verdict.id.clone(),
            new_verdict_id: new_verdict.id.clone(),
            new_decision: new_verdict.decision,
            hard_stops: new_verdict.hard_stops.clone(),
            receipts_count: receipts.len(),
        })
    }
}

fn build_auto_rejudge_entry(verdict: &VibeGateVerdict, old_verdict_id: &str) -> LaunchLedgerEntry {
    let mut payload =
        serde_json::to_value(verdict).expect("VibeGateVerdict serializes to JSON value");
    if let serde_json::Value::Object(map) = &mut payload {
        map.insert("wave_scope".into(), json!("auto_rejudge"));
        map.insert("supersedes".into(), json!(old_verdict_id));
    }
    LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: format!("ll_{}", verdict.id),
        kind: LedgerKind::VerdictIssued,
        subject_id: verdict.id.clone(),
        repo: Some(verdict.repo.clone()),
        payload,
        recorded_at: verdict.created_at,
        actor: "auto_rejudge".into(),
        signature: Signature::default_unsigned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{EvidenceInputs, build_evidence_pack};
    use crate::ledger::MemoryLedger;
    use crate::policy_yaml::fixtures;
    use crate::seam::LedgerFilter;
    use crate::types::*;
    use crate::verdict_store::MemoryVerdictStore;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};

    fn signed_pack(repo: &str) -> EvidencePack {
        let (h, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(40));
        let mut p = build_evidence_pack(EvidenceInputs {
            repo,
            source_branch: "jeryu-pr-1",
            target_branch: "main",
            head_sha: &h,
            base_sha: &b,
            policy_sha: &c,
            author_agent: Some("builder.x"),
            intent_id: None,
            risk: RiskTier::R2,
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
            gate_receipts: vec![],
        });
        p.signature = Some(Signature {
            key_id: "evidence-builder.v1".into(),
            algo: "ed25519".into(),
            value: "0".repeat(128),
        });
        p
    }

    fn receipt(role: ReviewerRole, agent: &str, pack: &EvidencePack) -> AgentApprovalReceipt {
        AgentApprovalReceipt {
            schema: SchemaTag::new(),
            id: format!("aar_{agent}"),
            evidence_pack_id: pack.id.clone(),
            role,
            agent_id: agent.into(),
            prompt_sha: None,
            provider: None,
            model: None,
            temperature: None,
            seed: None,
            raw_response_sha: Some("sha256:beef".into()),
            head_sha: pack.head_sha.clone(),
            policy_sha: pack.policy_sha.clone(),
            decision: ReviewDecision::Pass,
            reason: None,
            findings: vec![],
            not_author: true,
            tokens: TokenCounts::default(),
            created_at: Utc::now(),
            signature: Signature::stub(),
        }
    }

    /// A controllable evidence source.
    struct FakeEvidence {
        roles_to_pass: Vec<ReviewerRole>,
        build_fails: bool,
        reviews_fail: bool,
    }

    #[async_trait]
    impl EvidenceSource for FakeEvidence {
        async fn build_pack(&self, repo: &str, _pr_id: &str) -> SeamResult<EvidencePack> {
            if self.build_fails {
                return Err(SeamError::new("forge", "diff fetch failed"));
            }
            Ok(signed_pack(repo))
        }
        async fn run_reviews(
            &self,
            pack: &EvidencePack,
            _required: &[ReviewerRole],
        ) -> SeamResult<Vec<AgentApprovalReceipt>> {
            if self.reviews_fail {
                return Err(SeamError::new("orchestrator", "reviewer outage"));
            }
            Ok(self
                .roles_to_pass
                .iter()
                .map(|r| receipt(*r, &format!("{r:?}.v1"), pack))
                .collect())
        }
    }

    fn old_verdict(repo: &str) -> VibeGateVerdict {
        let now = Utc::now();
        VibeGateVerdict {
            schema: SchemaTag::new(),
            id: "vgv_old".into(),
            evidence_pack_id: "ep_old".into(),
            pull_request: Some("1".into()),
            repo: repo.into(),
            target_branch: "main".into(),
            head_sha: "0".repeat(40),
            policy_sha: "c".repeat(40),
            evidence_pack_digest: "sha256:old".into(),
            risk: RiskTier::R2,
            hard_stops: vec![],
            required_reviews: vec![],
            approval_receipts: vec![],
            decision: GateDecision::AllowMerge,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at: now - Duration::minutes(1),
            created_at: now - Duration::minutes(61),
            signature: Signature::stub(),
        }
    }

    fn service(
        evidence: FakeEvidence,
    ) -> (
        AutoRejudgeService,
        Arc<MemoryLedger>,
        Arc<MemoryVerdictStore>,
    ) {
        let ledger = Arc::new(MemoryLedger::new());
        let store = Arc::new(MemoryVerdictStore::new());
        let svc = AutoRejudgeService::new(
            Arc::new(evidence),
            store.clone(),
            ledger.clone(),
            Arc::new(EdSigningKey::generate("judge.v1")),
            Arc::new(fixtures::default_bundle()),
        );
        (svc, ledger, store)
    }

    #[tokio::test]
    async fn one_cycle_produces_one_verdict_one_ledger_entry_and_saves() {
        let evidence = FakeEvidence {
            roles_to_pass: vec![ReviewerRole::Security, ReviewerRole::TestIntegrity],
            build_fails: false,
            reviews_fail: false,
        };
        let (svc, ledger, store) = service(evidence);
        let old = old_verdict("owner/repo");
        store.save(&old).await.unwrap();

        let out = svc.rejudge("owner/repo", "1", &old).await.unwrap();
        assert_eq!(
            out.new_decision,
            GateDecision::AllowMerge,
            "quorum met → AllowMerge"
        );
        assert_eq!(out.receipts_count, 2);

        // Exactly one new VerdictIssued ledger entry, ed25519-signed.
        let entries = ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::VerdictIssued),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].signature.algo, "ed25519");
        assert_eq!(entries[0].payload["wave_scope"], "auto_rejudge");

        // The new verdict superseded the old one for (repo, pr).
        let latest = store
            .load_latest("owner/repo", Some("1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, out.new_verdict_id);
        assert_ne!(latest.id, old.id);
    }

    #[tokio::test]
    async fn orchestrator_failure_degrades_to_require_human_not_abort() {
        let evidence = FakeEvidence {
            roles_to_pass: vec![],
            build_fails: false,
            reviews_fail: true, // reviewer outage
        };
        let (svc, ledger, _store) = service(evidence);
        let old = old_verdict("owner/repo");
        let out = svc.rejudge("owner/repo", "1", &old).await.unwrap();
        // No receipts → insufficient quorum at R2 → RequireHuman (not an abort).
        assert_eq!(out.new_decision, GateDecision::RequireHuman);
        assert_eq!(out.receipts_count, 0);
        let entries = ledger.list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(entries.len(), 1, "still produced one signed ledger entry");
    }

    #[tokio::test]
    async fn pack_builder_failure_bubbles_up() {
        let evidence = FakeEvidence {
            roles_to_pass: vec![],
            build_fails: true,
            reviews_fail: false,
        };
        let (svc, _ledger, _store) = service(evidence);
        let old = old_verdict("owner/repo");
        let err = svc.rejudge("owner/repo", "1", &old).await.unwrap_err();
        assert_eq!(err.source, "forge");
    }
}
