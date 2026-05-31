//! Request/response value types for the typed AgentBridge operations.

use jeryu_core::phase7::{AgentId, AgentScope, ProofWitness, PullRequestId, ReceiptId, RepoId};
use jeryu_proof::{ProofEvidence, ProofPlan};

/// Agent context response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentContext {
    /// Repository id.
    pub repo: RepoId,
    /// Pull request id.
    pub pr: PullRequestId,
    /// Base SHA.
    pub base_sha: String,
    /// Head SHA.
    pub head_sha: String,
    /// Changed paths.
    pub changed_paths: Vec<String>,
}

/// Mergeability response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mergeability {
    /// Whether merge is currently allowed.
    pub mergeable: bool,
    /// Blockers explaining why merge is denied.
    pub blockers: Vec<String>,
}

/// One file patch in a dry-run request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilePatch {
    /// Path to mutate.
    pub path: String,
    /// Patch body. The dry-run path records this as metadata and scopes it
    /// against the agent's write policy; it is never committed to the tree.
    pub patch: String,
}

/// Dry-run patch request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DryRunPatchRequest {
    /// Agent scope.
    pub scope: AgentScope,
    /// PR id.
    pub pr: PullRequestId,
    /// Base SHA the patch is bound to.
    pub base_sha: String,
    /// Patch entries.
    pub patches: Vec<FilePatch>,
}

/// Dry-run patch response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DryRunPatchResponse {
    /// Receipt id proving the dry-run was scoped.
    pub receipt_id: ReceiptId,
    /// Changed paths.
    pub changed_paths: Vec<String>,
}

/// Proof plan request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofPlanRequest {
    /// Agent id.
    pub agent: AgentId,
    /// PR id.
    pub pr: PullRequestId,
    /// Changed paths.
    pub changed_paths: Vec<String>,
    /// Head SHA.
    pub head_sha: String,
}

/// Run proof request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunProofRequest {
    /// Agent id.
    pub agent: AgentId,
    /// Proof plan.
    pub plan: ProofPlan,
    /// Evidence. Tests may provide explicit evidence; empty evidence means no proof.
    pub evidence: Vec<ProofEvidence>,
}

/// Proposed fix request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposedFixRequest {
    /// Agent id.
    pub agent: AgentId,
    /// PR id.
    pub pr: PullRequestId,
    /// Dry-run receipt id.
    pub dry_run_receipt_id: ReceiptId,
    /// Proof witness.
    pub proof_witness: ProofWitness,
    /// Residual risk statement.
    pub residual_risk: String,
}

/// Hotfix request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HotfixRequest {
    /// Agent id.
    pub agent: AgentId,
    /// Repository id.
    pub repo: RepoId,
    /// Production tag.
    pub production_tag: String,
    /// Changed paths.
    pub changed_paths: Vec<String>,
    /// Dry-run receipt id.
    pub dry_run_receipt_id: ReceiptId,
    /// Proof witness.
    pub proof_witness: ProofWitness,
}
