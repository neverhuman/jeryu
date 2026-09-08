//! Proposal operations: agent-proposed fixes and narrow hotfixes, each gated
//! on a prior dry-run receipt + proof witness.

use super::AgentBridge;
use super::types::{HotfixRequest, ProposedFixRequest};
use jeryu_core::phase7::{JeryuError, JeryuResult, Receipt, ReceiptId, ReceiptKind};

impl AgentBridge {
    /// `POST /api/agent/propose-fix`.
    pub fn propose_fix(&mut self, request: ProposedFixRequest) -> JeryuResult<ReceiptId> {
        let pr_obj = self.require_pr(&request.pr)?;
        let Some(dry_run_receipt) = self.state.receipt(&request.dry_run_receipt_id) else {
            return Err(JeryuError::MissingReceipt(format!(
                "agent patch requires receipt {}",
                request.dry_run_receipt_id
            )));
        };
        if dry_run_receipt.kind != ReceiptKind::AgentDryRunPatch {
            return Err(JeryuError::MissingReceipt(format!(
                "receipt {} is not an agent dry-run patch receipt",
                request.dry_run_receipt_id
            )));
        }
        if request.proof_witness.pr != request.pr
            || request.proof_witness.head_sha != pr_obj.head_sha
        {
            return Err(JeryuError::MissingProofWitness(
                "proposed fix proof witness does not cover PR head".to_string(),
            ));
        }
        let receipt = Receipt::new(
            ReceiptKind::AgentProposedFix,
            pr_obj.repo.clone(),
            Some(request.agent),
            request.pr.to_string(),
            pr_obj.head_sha.clone(),
            format!(
                "agent proposed fix accepted with proof witness {}",
                request.proof_witness.id
            ),
            vec!["jeryu_agentbridge.propose_fix".to_string()],
            request.residual_risk,
        );
        let id = receipt.id.clone();
        self.state.add_receipt(receipt);
        Ok(id)
    }

    /// `POST /api/agent/hotfix`.
    pub fn hotfix(&mut self, request: HotfixRequest) -> JeryuResult<ReceiptId> {
        let Some(dry_run_receipt) = self.state.receipt(&request.dry_run_receipt_id) else {
            return Err(JeryuError::MissingReceipt(format!(
                "hotfix requires dry-run receipt {}",
                request.dry_run_receipt_id
            )));
        };
        if dry_run_receipt.repo != request.repo {
            return Err(JeryuError::MissingReceipt(
                "dry-run receipt repo mismatch".to_string(),
            ));
        }
        if request.changed_paths.len() > 5 {
            return Err(JeryuError::PolicyDenied(
                "hotfix must stay narrow: max 5 paths".to_string(),
            ));
        }
        let witness_paths = request
            .proof_witness
            .changed_paths
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let request_paths = request
            .changed_paths
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if witness_paths != request_paths {
            return Err(JeryuError::MissingProofWitness(
                "hotfix proof witness does not cover requested paths".to_string(),
            ));
        }
        let receipt = Receipt::new(
            ReceiptKind::AgentHotfix,
            request.repo,
            Some(request.agent),
            request.production_tag,
            request.proof_witness.head_sha,
            format!("hotfix accepted for {} path(s)", request_paths.len()),
            vec!["jeryu_agentbridge.hotfix".to_string()],
            "rollback required if production smoke fails",
        );
        let id = receipt.id.clone();
        self.state.add_receipt(receipt);
        Ok(id)
    }
}
