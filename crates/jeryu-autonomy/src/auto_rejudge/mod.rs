//! Auto-rejudge service — composition of the Evidence-Gate pieces.
//!
//! Invariants:
//!   - One rejudge run = one fresh `EvidencePack` build, one orchestrated
//!     reviewer pass, one pure `judge()` fusion, one signed `VerdictIssued`
//!     ledger entry, and one verdict save+supersede pair.
//!   - The new verdict supersedes the prior one for the same (repo, pull_request)
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
    pub prior_verdict_id: String,
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
        prior_verdict: &VibeGateVerdict,
    ) -> SeamResult<RejudgeOutcome> {
        // 1. Fresh signed evidence pack. A failure here is structural — bubble up.
        let pack = self.evidence.build_pack(repo, pr_id).await?;

        // 2. Required reviewer roles for the pack's risk tier.
        let required_roles: Vec<ReviewerRole> = self
            .policy
            .quorum_for(pack.risk)
            .map_or_else(Vec::new, |q| q.roles.clone());

        // 3. Run the orchestrator. A reviewer-orchestration Err is deliberately
        //    degraded to "no receipts" rather than bubbled up: a missing reviewer
        //    is itself a signal, and judge() converts the empty-receipt case into
        //    an insufficient-quorum RequireHuman escalation (fail-safe, never a
        //    silent AllowMerge). The `unwrap_or_else` makes the degradation an
        //    intentional, documented branch rather than a swallowed default.
        let receipts = self
            .evidence
            .run_reviews(&pack, &required_roles)
            .await
            .unwrap_or_else(|_orchestration_failed| {
                // No receipts -> insufficient quorum -> RequireHuman (see invariants).
                Vec::new()
            });

        // 4. Pure policy fusion. No side effects in judge().
        let outcome = judge(JudgeInputs {
            pack: &pack,
            receipts: &receipts,
            policy: &self.policy,
            repo,
            target_branch: &prior_verdict.target_branch,
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
        let mut entry = build_auto_rejudge_entry(&new_verdict, &prior_verdict.id);
        sign_entry(&mut entry, &self.signing_key);
        self.ledger
            .append(&entry)
            .await
            .map_err(|e| SeamError::new("auto_rejudge", format!("append VerdictIssued: {e}")))?;

        Ok(RejudgeOutcome {
            repo: repo.to_string(),
            pr_id: pr_id.to_string(),
            prior_verdict_id: prior_verdict.id.clone(),
            new_verdict_id: new_verdict.id.clone(),
            new_decision: new_verdict.decision,
            hard_stops: new_verdict.hard_stops.clone(),
            receipts_count: receipts.len(),
        })
    }
}

fn build_auto_rejudge_entry(
    verdict: &VibeGateVerdict,
    prior_verdict_id: &str,
) -> LaunchLedgerEntry {
    let mut payload =
        serde_json::to_value(verdict).expect("VibeGateVerdict serializes to JSON value");
    if let serde_json::Value::Object(map) = &mut payload {
        map.insert("wave_scope".into(), json!("auto_rejudge"));
        map.insert("supersedes".into(), json!(prior_verdict_id));
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
mod tests;
