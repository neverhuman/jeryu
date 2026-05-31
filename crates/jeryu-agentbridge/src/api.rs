//! Typed AgentBridge operations.

use crate::state::AgentBridgeState;
use jeryu_core::phase7::{
    AgentId, AgentScope, ChangedPath, JeryuError, JeryuResult, ProofWitness, PullRequest,
    PullRequestId, Receipt, ReceiptId, ReceiptKind, RepoId,
};
use jeryu_proof::{ChangeSet, ProofBlocker, ProofEngine, ProofEvidence, ProofPlan};

/// AgentBridge API object.
#[derive(Clone, Debug)]
pub struct AgentBridge {
    state: AgentBridgeState,
    proof_engine: ProofEngine,
}

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

impl AgentBridge {
    /// Creates an AgentBridge using the supplied proof engine.
    pub fn new(proof_engine: ProofEngine) -> Self {
        Self {
            state: AgentBridgeState::new(),
            proof_engine,
        }
    }

    /// Inserts or replaces a PR for this in-memory API.
    pub fn upsert_pr(&mut self, pr: PullRequest) {
        self.state.upsert_pr(pr);
    }

    /// Resolve a PR by id or fail with a single, explicit `NotFound`.
    ///
    /// Every typed endpoint that needs the PR routes through here so the
    /// missing-PR error is produced in exactly one place rather than repeated as
    /// an inline `ok_or_else` at each call site.
    fn require_pr(&self, pr: &PullRequestId) -> JeryuResult<&PullRequest> {
        self.state
            .pr(pr)
            .ok_or_else(|| JeryuError::NotFound(format!("PR {pr}")))
    }

    /// `GET /api/agent/context?repo=&pr=`.
    pub fn context(&self, repo: &RepoId, pr: &PullRequestId) -> JeryuResult<AgentContext> {
        let pr_obj = self.require_pr(pr)?;
        if &pr_obj.repo != repo {
            return Err(JeryuError::NotFound(format!(
                "PR {pr} not found in repo {repo}"
            )));
        }
        Ok(AgentContext {
            repo: pr_obj.repo.clone(),
            pr: pr_obj.id.clone(),
            base_sha: pr_obj.base_sha.clone(),
            head_sha: pr_obj.head_sha.clone(),
            changed_paths: pr_obj
                .changed_paths
                .iter()
                .map(|path| path.path.clone())
                .collect(),
        })
    }

    /// `GET /api/agent/mergeability?pr=`.
    pub fn mergeability(&self, pr: &PullRequestId) -> JeryuResult<Mergeability> {
        let pr_obj = self.require_pr(pr)?;
        let plan = self.proof_engine.plan(&ChangeSet {
            repo: pr_obj.repo.clone(),
            pr: pr_obj.id.clone(),
            head_sha: pr_obj.head_sha.clone(),
            paths: pr_obj.changed_paths.clone(),
            agent_authored: false,
        });
        match plan {
            Ok(_) => Ok(Mergeability {
                mergeable: false,
                blockers: vec!["proof witness required before queue admission".to_string()],
            }),
            Err(blocker) => Ok(Mergeability {
                mergeable: false,
                blockers: vec![blocker.message()],
            }),
        }
    }

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

    /// `GET /api/agent/receipts/{id}`.
    pub fn receipt(&self, id: &ReceiptId) -> JeryuResult<Receipt> {
        self.state
            .receipt(id)
            .cloned()
            .ok_or_else(|| JeryuError::NotFound(format!("receipt {id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_ci_scheduler::{DeterministicValidator, MergeQueue};
    use jeryu_core::phase7::{PullRequest, QueueEntryState};
    use jeryu_proof::default_phase7_engine;

    fn setup_bridge() -> (AgentBridge, PullRequest) {
        let mut bridge = AgentBridge::new(default_phase7_engine());
        let pr = PullRequest::new(
            RepoId::new("repo_phase7"),
            PullRequestId::new("pr_agent"),
            "agent/fix",
            "main",
            "base001",
            "head001",
            vec![ChangedPath::new("crates/jeryu_agentbridge/src/api.rs")],
        );
        bridge.upsert_pr(pr.clone());
        (bridge, pr)
    }

    fn evidence_for(plan: &ProofPlan) -> Vec<ProofEvidence> {
        plan.lanes
            .iter()
            .map(|lane| ProofEvidence {
                lane: lane.name.clone(),
                commands: lane.commands.clone(),
                success: true,
                log_digest: format!("digest:{}", lane.name),
            })
            .collect()
    }

    #[test]
    fn agent_broad_write_denied() {
        let (mut bridge, pr) = setup_bridge();
        let request = DryRunPatchRequest {
            scope: AgentScope {
                agent: AgentId::new("agent_repair"),
                repo: pr.repo.clone(),
                allowed_paths: vec!["crates/jeryu_agentbridge/".to_string()],
                max_paths: 2,
            },
            pr: pr.id.clone(),
            base_sha: pr.head_sha.clone(),
            patches: vec![FilePatch {
                path: "crates/jeryu_proof/src/engine.rs".to_string(),
                patch: "replace proof".to_string(),
            }],
        };
        let err = bridge
            .dry_run_patch(request)
            .expect_err("out-of-scope write must be denied");
        assert!(err.to_string().contains("out-of-scope"));
    }

    #[test]
    fn agent_patch_requires_receipt() {
        let (mut bridge, pr) = setup_bridge();
        let plan = bridge
            .proof_plan(ProofPlanRequest {
                agent: AgentId::new("agent_repair"),
                pr: pr.id.clone(),
                changed_paths: vec!["crates/jeryu_agentbridge/src/api.rs".to_string()],
                head_sha: pr.head_sha.clone(),
            })
            .expect("proof plan");
        let witness = bridge
            .run_proof(RunProofRequest {
                agent: AgentId::new("agent_repair"),
                plan: plan.clone(),
                evidence: evidence_for(&plan),
            })
            .expect("witness");
        let err = bridge
            .propose_fix(ProposedFixRequest {
                agent: AgentId::new("agent_repair"),
                pr: pr.id.clone(),
                dry_run_receipt_id: ReceiptId::new("receipt_missing"),
                proof_witness: witness,
                residual_risk: "none".to_string(),
            })
            .expect_err("proposal without dry-run receipt must fail");
        assert!(err.to_string().contains("agent patch requires receipt"));
    }

    #[test]
    fn agents_can_fix_failing_pr_through_typed_apis_only() {
        let (mut bridge, pr) = setup_bridge();
        let context = bridge.context(&pr.repo, &pr.id).expect("context");
        assert_eq!(context.head_sha, "head001");

        let dry_run = bridge
            .dry_run_patch(DryRunPatchRequest {
                scope: AgentScope {
                    agent: AgentId::new("agent_repair"),
                    repo: pr.repo.clone(),
                    allowed_paths: vec!["crates/jeryu_agentbridge/".to_string()],
                    max_paths: 2,
                },
                pr: pr.id.clone(),
                base_sha: pr.head_sha.clone(),
                patches: vec![FilePatch {
                    path: "crates/jeryu_agentbridge/src/api.rs".to_string(),
                    patch: "minimal fix".to_string(),
                }],
            })
            .expect("scoped dry-run succeeds");

        let plan = bridge
            .proof_plan(ProofPlanRequest {
                agent: AgentId::new("agent_repair"),
                pr: pr.id.clone(),
                changed_paths: dry_run.changed_paths.clone(),
                head_sha: pr.head_sha.clone(),
            })
            .expect("plan");
        let witness = bridge
            .run_proof(RunProofRequest {
                agent: AgentId::new("agent_repair"),
                plan: plan.clone(),
                evidence: evidence_for(&plan),
            })
            .expect("witness");
        let fix_receipt = bridge
            .propose_fix(ProposedFixRequest {
                agent: AgentId::new("agent_repair"),
                pr: pr.id.clone(),
                dry_run_receipt_id: dry_run.receipt_id,
                proof_witness: witness.clone(),
                residual_risk: "none".to_string(),
            })
            .expect("fix receipt");
        assert!(bridge.receipt(&fix_receipt).is_ok());

        let mut queue = MergeQueue::new();
        queue.enqueue(pr, Some(witness)).expect("queue admission");
        let summary = queue.process_all(&DeterministicValidator);
        assert_eq!(summary.mergeable, 1);
        assert_eq!(queue.entries()[0].state, QueueEntryState::Mergeable);
    }
}
