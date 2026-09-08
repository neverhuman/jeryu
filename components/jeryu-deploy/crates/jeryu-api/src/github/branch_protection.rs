//! Branch-protection routes
//! (`/repos/{owner}/{repo}/branches/{branch}/protection`) and their
//! GitHub-shaped renderer.

use jeryu_core::{BranchProtectionRule, SetBranchProtectionRequest};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{error_response, json_response, parse_body};

impl GithubRouter {
    pub(super) fn get_protection(&self, owner: &str, repo: &str, branch: &str) -> Response {
        match self.core.get_branch_protection(owner, repo, branch) {
            Ok(rule) => json_response(200, &branch_protection_json(&rule)),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn set_protection(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        body: &str,
    ) -> Response {
        let req: SetBranchProtectionRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.set_branch_protection(owner, repo, branch, req) {
            Ok(rule) => json_response(200, &branch_protection_json(&rule)),
            Err(err) => error_response(err),
        }
    }
}

fn branch_protection_json(rule: &BranchProtectionRule) -> Value {
    json!({
        "url": format!(
            "/repos/{}/{}/branches/{}/protection",
            rule.owner, rule.repo, rule.branch
        ),
        "required_status_checks": {
            "strict": rule.required_linear_history,
            "contexts": rule.required_status_checks,
        },
        "required_pull_request_reviews": {
            "required_approving_review_count": rule.required_approving_review_count,
        },
        "enforce_admins": { "enabled": rule.enforce_admins },
        "required_linear_history": { "enabled": rule.required_linear_history },
        "allow_force_pushes": { "enabled": rule.allow_force_pushes },
        "allow_deletions": { "enabled": rule.allow_deletions },
        "required_signatures": { "enabled": rule.require_signed_commits },
        "required_jankurai_proof": { "enabled": rule.require_jankurai_proof },
        "updated_at": rule.updated_at,
    })
}
