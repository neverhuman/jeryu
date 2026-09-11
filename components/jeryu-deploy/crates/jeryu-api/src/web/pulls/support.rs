//! Shared pull-request BFF helpers extracted from the route module.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::{ForgeError, PullRequest};
use jeryu_readmodel::contracts::{
    AgentPosture, AvailableAction, CheckPosture, EntityHandle, MergePassportStatus, Mergeability,
    PullRequestDetail, PullRequestState as WebPullRequestState, PullRequestSummary,
};
use serde_json::{Value, json};

use super::super::repositories::{find_repo, repo_id};
use super::super::{WebState, server_time};
use super::{DOCS_URL, PROOF_LANE, RequiredContextPosture, RequiredContextState};

pub(super) fn resolve_pr(
    state: &WebState,
    id: &str,
    number: u64,
) -> Option<(jeryu_core::Repository, PullRequest)> {
    let repo = find_repo(state, id)?;
    let pr = state
        .github
        .core()
        .get_pull_request(&repo.owner, &repo.name, number)
        .ok()?;
    Some((repo, pr))
}

pub(super) fn self_approval_forbidden(pr: &PullRequest, reviewer: &str) -> Option<AxumResponse> {
    if pr.author != reviewer {
        return None;
    }
    Some(repair_error(
        StatusCode::FORBIDDEN,
        "pull_self_approval_forbidden",
        "approve pull request",
        "pull request authors cannot approve their own changes",
        &[
            "request approval from an authenticated reviewer distinct from the pull request author",
            "retry with the same expected head after the independent reviewer signs in",
        ],
        PROOF_LANE,
        Some(json!({
            "pull_number": pr.number,
            "author": pr.author,
            "reviewer": reviewer,
        })),
    ))
}

pub(super) fn state_matches(pr: &PullRequest, filter: Option<&str>) -> bool {
    match filter.unwrap_or("all") {
        "open" => {
            !pr.merged
                && !matches!(
                    pr.state,
                    jeryu_core::PullRequestState::Closed | jeryu_core::PullRequestState::Merged
                )
        }
        "closed" => matches!(pr.state, jeryu_core::PullRequestState::Closed),
        "merged" => pr.merged || matches!(pr.state, jeryu_core::PullRequestState::Merged),
        "all" => true,
        _ => true,
    }
}

pub(super) fn detail_for_pr(
    state: &WebState,
    pr: &PullRequest,
    authenticated_login: Option<&str>,
) -> PullRequestDetail {
    let required_contexts = required_contexts(state, pr);
    detail_for_pr_with_required_contexts(state, pr, &required_contexts, authenticated_login)
}

#[cfg(test)]
pub(in crate::web) fn detail_for_pr_with_audit_enforcement(
    state: &WebState,
    pr: &PullRequest,
    audit_enforce_merge: bool,
) -> PullRequestDetail {
    let required_contexts = required_contexts_with_enforcement(state, pr, audit_enforce_merge);
    detail_for_pr_with_required_contexts(state, pr, &required_contexts, None)
}

fn detail_for_pr_with_required_contexts(
    state: &WebState,
    pr: &PullRequest,
    required_contexts: &[RequiredContextPosture],
    authenticated_login: Option<&str>,
) -> PullRequestDetail {
    let mut summary = summary_with_required_contexts(state, pr, required_contexts);
    let merge_passport = passport(&summary, pr, required_contexts);
    let reviews = reviews_for_pr(state, pr);
    summary.review.user_review_state = authenticated_login.and_then(|login| {
        reviews
            .iter()
            .find(|review| review.effective && review.author == login)
            .map(|review| review.state.clone())
    });
    PullRequestDetail {
        passport_hash: summary.passport_hash.clone(),
        summary,
        description: pr.body.clone(),
        head_tree_sha: commit_tree_sha(state, pr, &pr.head.sha),
        base_tree_sha: commit_tree_sha(state, pr, &pr.base.sha),
        reviews,
        merge_passport,
    }
}

pub(super) fn summary(state: &WebState, pr: &PullRequest) -> PullRequestSummary {
    let required_contexts = required_contexts(state, pr);
    summary_with_required_contexts(state, pr, &required_contexts)
}

fn summary_with_required_contexts(
    state: &WebState,
    pr: &PullRequest,
    required_contexts: &[RequiredContextPosture],
) -> PullRequestSummary {
    let repo = find_repo(state, &format!("{}/{}", pr.owner, pr.repo))
        .expect("PR owner/repo must resolve to a repository");
    let checks = checks_for_pr(state, pr);
    let review = review_posture(state, pr);
    let web_state = web_pr_state(pr);
    let mergeable = !pr.draft
        && !pr.merged
        && matches!(web_state, WebPullRequestState::Open)
        && pr.mergeable
        && required_contexts
            .iter()
            .all(|context| context.state == RequiredContextState::Passing)
        && review.approvals >= review.required_approvals
        && review.changes_requested == 0
        && review.unresolved_threads == 0;
    let reason = if mergeable {
        None
    } else if pr.draft {
        Some("draft pull request".to_string())
    } else if let Some(context) = required_contexts
        .iter()
        .find(|context| context.state != RequiredContextState::Passing)
    {
        Some(format!(
            "required context {} is {}",
            context.name,
            context.state.wire_name()
        ))
    } else if review.changes_requested > 0 {
        Some("changes requested on the current head".to_string())
    } else if review.approvals < review.required_approvals {
        Some("required approvals missing".to_string())
    } else if review.unresolved_threads > 0 {
        Some("unresolved review threads".to_string())
    } else if !pr.mergeable {
        Some(pr.mergeable_state.clone())
    } else {
        None
    };
    let status = if mergeable {
        MergePassportStatus::Pass
    } else {
        MergePassportStatus::Blocked
    };
    let blockers = passport_blockers(required_contexts, &review, pr);
    let passport_hash = passport_hash(
        state,
        pr,
        status.clone(),
        &blockers,
        &review,
        required_contexts,
    );
    PullRequestSummary {
        repo: repo_id(&repo),
        number: pr.number as u32,
        entity: EntityHandle {
            kind: "pull_request".to_string(),
            id: format!("{}#{}", repo.id, pr.number),
        },
        title: pr.title.clone(),
        author: pr.author.clone(),
        head_ref: pr.head.ref_name.clone(),
        base_ref: pr.base.ref_name.clone(),
        head_sha: pr.head.sha.clone(),
        base_sha: pr.base.sha.clone(),
        state: web_state,
        draft: pr.draft,
        mergeable: Mergeability {
            level: if mergeable { "mergeable" } else { "blocked" }.to_string(),
            can_merge: mergeable,
            reason,
            exact_head_sha: pr.head.sha.clone(),
            required_gate: if mergeable {
                None
            } else {
                Some("merge_passport".to_string())
            },
        },
        review,
        checks: CheckPosture {
            total: checks.total,
            passing: checks.passing,
            failing: checks.failing,
            pending: checks.pending,
            skipped: checks.skipped,
        },
        agents: AgentPosture {
            active_sessions: 0,
            proposed_patches: 0,
            evidence_packets: 0,
            blockers: 0,
        },
        labels: Vec::new(),
        updated_at: pr.updated_at.to_rfc3339(),
        passport_hash: Some(passport_hash),
        available_actions: vec![
            AvailableAction {
                action_id: "pull.approve".to_string(),
                label: "Approve".to_string(),
                risk: None,
            },
            AvailableAction {
                action_id: "pull.merge".to_string(),
                label: "Merge".to_string(),
                risk: Some("medium".to_string()),
            },
        ],
    }
}

#[cfg(test)]
use super::posture::required_contexts_with_enforcement;
use super::posture::{
    checks_for_pr, commit_tree_sha, passport, passport_blockers, passport_hash, required_contexts,
    review_posture, reviews_for_pr, web_pr_state,
};

pub(super) fn not_found(purpose: &'static str, message: &str) -> AxumResponse {
    repair_error(
        StatusCode::NOT_FOUND,
        "not_found",
        purpose,
        message,
        &[
            "verify the repository id and pull request number",
            "refresh the local forge import before retrying",
        ],
        PROOF_LANE,
        None,
    )
}

pub(super) fn stale_sha(expected: &str, current: &str) -> AxumResponse {
    repair_error(
        StatusCode::CONFLICT,
        "merge_sha_stale",
        "guard pull request mutation by exact head sha",
        "expected_head_sha does not match the current PR head",
        &[
            "refresh the PR detail and re-review the current head",
            "retry the mutation with the current expected_head_sha",
        ],
        PROOF_LANE,
        Some(json!({
            "expected_head_sha": expected,
            "current_head_sha": current,
        })),
    )
}

pub(super) fn core_error(error: ForgeError, purpose: &'static str) -> AxumResponse {
    match error {
        ForgeError::NotFound(reason) => not_found(purpose, &reason),
        ForgeError::Validation(reason) => repair_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            purpose,
            &reason,
            &[
                "check request fields before retrying",
                "add or rerun a boundary test for the rejected shape",
            ],
            PROOF_LANE,
            None,
        ),
        ForgeError::BranchProtection(reason) => repair_error(
            StatusCode::CONFLICT,
            "merge_blocked",
            purpose,
            &reason,
            &[
                "inspect branch protection and merge passport blockers",
                "supply required checks, approvals, or proof evidence",
            ],
            PROOF_LANE,
            None,
        ),
        ForgeError::Conflict(reason) => repair_error(
            StatusCode::CONFLICT,
            "conflict",
            purpose,
            &reason,
            &[
                "refresh the pull request before retrying",
                "recompute merge evidence for the current head",
            ],
            PROOF_LANE,
            None,
        ),
        ForgeError::Unauthenticated(reason) => repair_error(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            purpose,
            &reason,
            &["authenticate with a current credential before retrying"],
            PROOF_LANE,
            None,
        ),
        ForgeError::PreconditionRequired(reason) => repair_error(
            StatusCode::PRECONDITION_REQUIRED,
            "precondition_required",
            purpose,
            &reason,
            &["refresh the source and supply the required bound precondition"],
            PROOF_LANE,
            None,
        ),
        ForgeError::Forbidden(reason) => repair_error(
            StatusCode::FORBIDDEN,
            "forbidden",
            purpose,
            &reason,
            &[
                "authenticate as the permitted review actor",
                "preserve author and reviewer ownership rules",
            ],
            PROOF_LANE,
            None,
        ),
        ForgeError::WriterUnavailable(reason) => repair_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "writer_unavailable",
            purpose,
            &reason,
            &[
                "inspect the current writer and backing resource custody",
                "retry after writer ownership and storage identity are restored",
            ],
            PROOF_LANE,
            None,
        ),
        ForgeError::Storage(reason) => repair_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            purpose,
            &reason,
            &[
                "check the local SQLite store and filesystem permissions",
                "restart the local API after verifying storage health",
            ],
            PROOF_LANE,
            None,
        ),
    }
}

pub(super) fn github_merge_error(response: crate::Response, pr: &PullRequest) -> AxumResponse {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let repair_status = if status == StatusCode::METHOD_NOT_ALLOWED {
        StatusCode::CONFLICT
    } else {
        status
    };
    let message = serde_json::from_str::<Value>(&response.body)
        .ok()
        .and_then(|body| {
            body.get("message")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(response.body);
    let code = match status {
        StatusCode::METHOD_NOT_ALLOWED | StatusCode::CONFLICT => "merge_blocked",
        StatusCode::UNPROCESSABLE_ENTITY => "merge_unprocessable",
        StatusCode::NOT_FOUND => "not_found",
        _ => "merge_failed",
    };
    repair_error(
        repair_status,
        code,
        "merge pull request",
        &message,
        &[
            "inspect the merge passport blockers before retrying",
            "rerun required checks and collect approvals for the current head",
        ],
        PROOF_LANE,
        Some(json!({ "head_sha": pr.head.sha })),
    )
}

pub(super) fn repair_error(
    status: StatusCode,
    code: &'static str,
    purpose: &'static str,
    reason: &str,
    common_fixes: &'static [&'static str],
    repair_hint: &'static str,
    details: Option<Value>,
) -> AxumResponse {
    let error = json!({
        "code": code,
        "message": reason,
        "details": match details {
            Some(details) => details,
            None => json!({}),
        },
        "request_id": format!("pulls-{}", server_time()),
    });
    (
        status,
        Json(json!({
            "error": error,
            "code": code,
            "message": reason,
            "purpose": purpose,
            "reason": reason,
            "common_fixes": common_fixes,
            "docs_url": DOCS_URL,
            "repair_hint": repair_hint,
        })),
    )
        .into_response()
}
