//! Branch protection rules, CODEOWNERS, and ref-operation enforcement.

use chrono::Utc;

use super::{ForgeCore, evaluate_locked, require_name};
use crate::branch_protection::{
    BranchProtectionEvaluation, EvaluationContext, RefOperation, RefOperationEvaluation,
    evaluate_ref_operation,
};
use crate::errors::{ForgeError, Result};
use crate::model::*;

impl ForgeCore {
    pub fn set_branch_protection(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        request: SetBranchProtectionRequest,
    ) -> Result<BranchProtectionRule> {
        self.ensure_repo_exists(owner, repo)?;
        require_name("branch", branch)?;
        let rule = BranchProtectionRule {
            owner: owner.to_string(),
            repo: repo.to_string(),
            branch: branch.to_string(),
            required_status_checks: request.required_status_checks,
            required_approving_review_count: request.required_approving_review_count,
            enforce_admins: request.enforce_admins,
            required_linear_history: request.required_linear_history,
            allow_force_pushes: request.allow_force_pushes,
            allow_deletions: request.allow_deletions,
            require_signed_commits: request.require_signed_commits,
            require_jankurai_proof: request.require_jankurai_proof,
            updated_at: Utc::now(),
        };
        let mut state = self.state.write();
        let previous = state.clone();
        state.branch_protections.insert(
            (owner.to_string(), repo.to_string(), branch.to_string()),
            rule.clone(),
        );
        self.persist_after_mutation(&mut state, previous)?;
        Ok(rule)
    }

    pub fn get_branch_protection(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<BranchProtectionRule> {
        self.state
            .read()
            .branch_protections
            .get(&(owner.to_string(), repo.to_string(), branch.to_string()))
            .cloned()
            .ok_or_else(|| {
                ForgeError::NotFound(format!("branch protection {owner}/{repo}:{branch}"))
            })
    }

    /// Stores the repository's CODEOWNERS file contents. Used by branch
    /// protection to require code-owner approval of changed paths.
    pub fn set_codeowners(&self, owner: &str, repo: &str, contents: &str) -> Result<()> {
        self.ensure_repo_exists(owner, repo)?;
        let mut state = self.state.write();
        let previous = state.clone();
        state
            .codeowners
            .insert((owner.to_string(), repo.to_string()), contents.to_string());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(())
    }

    pub fn get_codeowners(&self, owner: &str, repo: &str) -> Result<String> {
        self.ensure_repo_exists(owner, repo)?;
        self.state
            .read()
            .codeowners
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("CODEOWNERS {owner}/{repo}")))
    }

    pub fn evaluate_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        sha: Option<&str>,
    ) -> Result<BranchProtectionEvaluation> {
        let state = self.state.read();
        let pr = state
            .pulls
            .get(&(owner.to_string(), repo.to_string(), number))
            .ok_or_else(|| ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}")))?;
        Ok(evaluate_locked(&state, pr, sha))
    }

    /// Evaluates a force-push or branch-deletion against the branch's
    /// protection rule. `actor_is_admin` lets an admin bypass when the rule's
    /// `enforce_admins` is off (GitHub's "Include administrators" toggle).
    pub fn evaluate_ref_operation(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        operation: RefOperation,
        actor_is_admin: bool,
    ) -> Result<RefOperationEvaluation> {
        self.ensure_repo_exists(owner, repo)?;
        let state = self.state.read();
        let protection = state.branch_protections.get(&(
            owner.to_string(),
            repo.to_string(),
            branch.to_string(),
        ));
        Ok(evaluate_ref_operation(
            operation,
            protection,
            EvaluationContext {
                codeowners: None,
                actor_is_admin,
            },
        ))
    }

    /// Attempts to force-push the branch, honoring `allow_force_pushes`.
    pub fn force_push(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        actor_is_admin: bool,
    ) -> Result<()> {
        let evaluation = self.evaluate_ref_operation(
            owner,
            repo,
            branch,
            RefOperation::ForcePush,
            actor_is_admin,
        )?;
        if evaluation.allowed {
            Ok(())
        } else {
            Err(ForgeError::BranchProtection(format!(
                "{:?}",
                evaluation.blockers
            )))
        }
    }

    /// Attempts to delete the branch ref, honoring `allow_deletions`.
    pub fn delete_ref(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        actor_is_admin: bool,
    ) -> Result<()> {
        let evaluation =
            self.evaluate_ref_operation(owner, repo, branch, RefOperation::Delete, actor_is_admin)?;
        if evaluation.allowed {
            Ok(())
        } else {
            Err(ForgeError::BranchProtection(format!(
                "{:?}",
                evaluation.blockers
            )))
        }
    }
}
