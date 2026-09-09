//! Typed AgentBridge operations.
//!
//! The [`AgentBridge`] API object hosts the in-memory PR/receipt state and a
//! [`ProofEngine`], and exposes the typed endpoints grouped by responsibility:
//!   - read-only context + mergeability + receipt lookup (this module),
//!   - dry-run + proof operations ([`dry_run`]),
//!   - the bounded scope-checked mutation seam ([`mutate`]),
//!   - proposal + hotfix operations ([`propose`]).
//!
//! All request/response value types live in [`types`] and are re-exported here.

mod dry_run;
mod mutate;
mod propose;
mod types;

pub use types::*;

use crate::state::AgentBridgeState;
use jeryu_core::phase7::{
    JeryuError, JeryuResult, PullRequest, PullRequestId, Receipt, ReceiptId, RepoId,
};
use jeryu_proof::{ChangeSet, ProofEngine};

/// AgentBridge API object.
#[derive(Clone, Debug)]
pub struct AgentBridge {
    state: AgentBridgeState,
    proof_engine: ProofEngine,
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
    use jeryu_core::phase7::{AgentId, AgentScope, ChangedPath, PullRequest, QueueEntryState};
    use jeryu_proof::{ProofEvidence, ProofPlan, default_phase7_engine};

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
