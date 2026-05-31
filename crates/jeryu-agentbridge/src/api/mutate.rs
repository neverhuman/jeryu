//! Bounded, scope-checked mutation seam.
//!
//! [`AgentBridge::apply_scoped_patch`] is the single public entry point an
//! external transport (e.g. the MCP `propose_patch` tool) uses to perform a
//! real, scope-validated bounded mutation. It folds two gates into one call:
//!   1. the agent write scope (`AgentScope::permits_all`: non-empty, under the
//!      mutation cap, every path within an allowed prefix), and
//!   2. the proof-engine ownership gate (every changed path must map to an
//!      owner rule; ownerless / out-of-tree paths are denied).
//!
//! On success it records an append-only [`ReceiptKind::AgentDryRunPatch`]
//! receipt and returns its id. The patch body is never committed to the tree.

use super::AgentBridge;
use super::types::{ScopedPatchRequest, ScopedPatchResponse};
use jeryu_core::phase7::{ChangedPath, JeryuError, JeryuResult, Receipt, ReceiptKind};
use jeryu_proof::ChangeSet;

impl AgentBridge {
    /// Apply a bounded, scope-checked patch through the agent write policy.
    ///
    /// Denies (without recording a receipt) when:
    ///   - the PR is unknown,
    ///   - the base SHA does not match the PR base/head,
    ///   - the patch set is empty, exceeds the scope cap, or touches a path
    ///     outside the agent's allowed prefixes (`AgentScope::permits_all`),
    ///   - the repo in scope does not match the PR repo, or
    ///   - the proof engine cannot plan the change (e.g. ownerless path).
    pub fn apply_scoped_patch(
        &mut self,
        request: ScopedPatchRequest,
    ) -> JeryuResult<ScopedPatchResponse> {
        let pr_obj = self.require_pr(&request.pr)?;
        if request.scope.repo != pr_obj.repo {
            return Err(JeryuError::PolicyDenied(format!(
                "agent {} scope repo {} does not match PR repo {}",
                request.scope.agent, request.scope.repo, pr_obj.repo
            )));
        }
        if request.base_sha != pr_obj.head_sha && request.base_sha != pr_obj.base_sha {
            return Err(JeryuError::Invalid(format!(
                "scoped patch base SHA {} does not match PR base/head",
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
            format!("agent scoped patch covers {} path(s)", paths.len()),
            vec!["jeryu_agentbridge.apply_scoped_patch".to_string()],
            "none: patch was scoped and not committed",
        );
        let receipt_id = receipt.id.clone();
        self.state.add_receipt(receipt);
        Ok(ScopedPatchResponse {
            receipt_id,
            changed_paths: paths,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::FilePatch;
    use jeryu_core::phase7::{
        AgentId, AgentScope, ChangedPath as PathEntry, PullRequest, PullRequestId, RepoId,
    };
    use jeryu_proof::default_phase7_engine;

    fn setup() -> (AgentBridge, PullRequest) {
        let mut bridge = AgentBridge::new(default_phase7_engine());
        let pr = PullRequest::new(
            RepoId::new("repo_phase7"),
            PullRequestId::new("pr_agent"),
            "agent/fix",
            "main",
            "base001",
            "head001",
            vec![PathEntry::new("crates/jeryu_agentbridge/src/api.rs")],
        );
        bridge.upsert_pr(pr.clone());
        (bridge, pr)
    }

    fn scope(pr: &PullRequest) -> AgentScope {
        AgentScope {
            agent: AgentId::new("agent_repair"),
            repo: pr.repo.clone(),
            allowed_paths: vec!["crates/jeryu_agentbridge/".to_string()],
            max_paths: 2,
        }
    }

    #[test]
    fn in_scope_patch_records_receipt() {
        let (mut bridge, pr) = setup();
        let resp = bridge
            .apply_scoped_patch(ScopedPatchRequest {
                scope: scope(&pr),
                pr: pr.id.clone(),
                base_sha: pr.head_sha.clone(),
                patches: vec![FilePatch {
                    path: "crates/jeryu_agentbridge/src/api.rs".to_string(),
                    patch: "minimal fix".to_string(),
                }],
            })
            .expect("in-scope patch must be accepted");
        assert_eq!(
            resp.changed_paths,
            vec!["crates/jeryu_agentbridge/src/api.rs"]
        );
        assert!(bridge.receipt(&resp.receipt_id).is_ok());
    }

    #[test]
    fn out_of_scope_patch_denied_without_receipt() {
        let (mut bridge, pr) = setup();
        let err = bridge
            .apply_scoped_patch(ScopedPatchRequest {
                scope: scope(&pr),
                pr: pr.id.clone(),
                base_sha: pr.head_sha.clone(),
                patches: vec![FilePatch {
                    path: "crates/jeryu_proof/src/engine.rs".to_string(),
                    patch: "replace proof".to_string(),
                }],
            })
            .expect_err("out-of-scope write must be denied");
        assert!(err.to_string().contains("out-of-scope"));
    }

    #[test]
    fn ownerless_path_denied() {
        let (mut bridge, pr) = setup();
        let mut wide_scope = scope(&pr);
        wide_scope.allowed_paths = vec![String::new()]; // allow anything by scope
        let err = bridge
            .apply_scoped_patch(ScopedPatchRequest {
                scope: wide_scope,
                pr: pr.id.clone(),
                base_sha: pr.head_sha.clone(),
                patches: vec![FilePatch {
                    path: "unknown/path.rs".to_string(),
                    patch: "edit".to_string(),
                }],
            })
            .expect_err("ownerless path must be denied by the proof gate");
        assert!(err.to_string().contains("ownerless"));
    }
}
