//! Dry-run + proof operations: scoped patch dry-runs, proof planning, and
//! proof verification.

use super::AgentBridge;
use super::types::{DryRunPatchRequest, DryRunPatchResponse, ProofPlanRequest, RunProofRequest};
use jeryu_core::phase7::{
    ChangedPath, JeryuError, JeryuResult, ProofWitness, Receipt, ReceiptKind,
};
use jeryu_proof::{ChangeSet, ProofBlocker, ProofPlan};

impl AgentBridge {
    /// `POST /api/agent/dry-run/patch`.
    pub fn dry_run_patch(
        &mut self,
        request: DryRunPatchRequest,
    ) -> JeryuResult<DryRunPatchResponse> {
        let pr_obj = self.require_pr(&request.pr)?;
        if request.base_sha != pr_obj.head_sha && request.base_sha != pr_obj.base_sha {
            return Err(JeryuError::Invalid(format!(
                "dry-run base SHA {} does not match PR base/head",
                request.base_sha
            )));
        }
        let paths = request
            .patches
            .iter()
            .map(|patch| patch.path.clone())
            .collect::<Vec<_>>();
        if !request.scope.permits_all(&paths) {
            return Err(JeryuError::PolicyDenied(format!(
                "agent {} attempted broad or out-of-scope write",
                request.scope.agent
            )));
        }
        let change_set = ChangeSet {
            repo: pr_obj.repo.clone(),
            pr: pr_obj.id.clone(),
            head_sha: pr_obj.head_sha.clone(),
            paths: paths
                .iter()
                .map(|path| ChangedPath::new(path.as_str()))
                .collect(),
            agent_authored: true,
        };
        if let Err(blocker) = self.proof_engine.plan(&change_set) {
            return Err(JeryuError::PolicyDenied(blocker.message()));
        }
        let receipt = Receipt::new(
            ReceiptKind::AgentDryRunPatch,
            pr_obj.repo.clone(),
            Some(request.scope.agent.clone()),
            request.pr.to_string(),
            pr_obj.head_sha.clone(),
            format!("agent dry-run patch covers {} path(s)", paths.len()),
            vec!["jeryu_agentbridge.dry_run_patch".to_string()],
            "none: patch was scoped and not committed",
        );
        let receipt_id = receipt.id.clone();
        self.state.add_receipt(receipt);
        Ok(DryRunPatchResponse {
            receipt_id,
            changed_paths: paths,
        })
    }

    /// `POST /api/agent/proof-plan`.
    pub fn proof_plan(&self, request: ProofPlanRequest) -> Result<ProofPlan, ProofBlocker> {
        let Some(pr_obj) = self.state.pr(&request.pr) else {
            return Err(ProofBlocker::OwnerlessPath(format!(
                "missing PR {}",
                request.pr
            )));
        };
        self.proof_engine.plan(&ChangeSet {
            repo: pr_obj.repo.clone(),
            pr: pr_obj.id.clone(),
            head_sha: request.head_sha,
            paths: request
                .changed_paths
                .iter()
                .map(|path| ChangedPath::new(path.as_str()))
                .collect(),
            agent_authored: true,
        })
    }

    /// `POST /api/agent/run-proof`.
    pub fn run_proof(&self, request: RunProofRequest) -> Result<ProofWitness, ProofBlocker> {
        let _ = request.agent;
        self.proof_engine.verify(&request.plan, &request.evidence)
    }
}
