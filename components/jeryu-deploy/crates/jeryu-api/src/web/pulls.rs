//! Pull request BFF routes for the SPA's W-FE-11 surface.
//!
//! These routes translate the local forge's authoritative pull request,
//! review, and check-run state into the typed web contracts consumed by the
//! React cockpit. Missing diff hunks or review threads are explicit empty
//! payloads derived from the PR metadata, never synthetic review content.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::{
    AccountSummary, CheckConclusion, CheckRun, CheckRunStatus, CommitStatusState,
    CreateReviewRequest, ForgeError, MergeBlocker,
    MergePullRequestRequest as CoreMergePullRequestRequest, PullRequest, ReviewCommentInput,
    ReviewState, check_conclusion_wire_value, effective_reviews_for_pull_request,
};
use jeryu_readmodel::contracts::{
    AgentPosture, AvailableAction, CheckPosture, CreateReviewCommentRequest,
    DismissPullReviewRequest, EntityHandle, MergePassport, MergePassportBlocker,
    MergePassportStatus, Mergeability, PullRequestDetail, PullRequestReview,
    PullRequestState as WebPullRequestState, PullRequestSummary, ReviewComment as WebReviewComment,
    ReviewPosture, ReviewThread, ReviewVerdict, SubmitReviewRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::repositories::{find_repo, repo_id};
use super::{WebState, server_time};

const DOCS_URL: &str = "docs/errors.md";
const PROOF_LANE: &str = "rerun cargo test -p jeryu-api --features web --jobs 40 pulls";

#[derive(Debug, Clone, Deserialize)]
pub(super) struct PullListQuery {
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PullRequestListResponse {
    items: Vec<PullRequestSummary>,
    total: usize,
}

#[derive(Debug, Clone, Serialize)]
struct PullRequestDiff {
    head_sha: String,
    base_sha: String,
    files: Vec<PullRequestDiffFile>,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PullRequestDiffFile {
    path: String,
    old_path: Option<String>,
    status: &'static str,
    additions: u32,
    deletions: u32,
    risk: Option<&'static str>,
    is_binary: bool,
    hunks: Vec<PullRequestDiffHunk>,
}

#[derive(Debug, Clone, Serialize)]
struct PullRequestDiffHunk {
    header: String,
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PullRequestChecks {
    total: u32,
    passing: u32,
    failing: u32,
    pending: u32,
    skipped: u32,
    checks: Vec<PullRequestCheck>,
}

#[derive(Debug, Clone, Serialize)]
struct PullRequestCheck {
    id: String,
    name: String,
    status: String,
    conclusion: Option<String>,
    details_url: Option<String>,
    description: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RequiredContextState {
    Missing,
    Failing,
    Pending,
    Passing,
}

impl RequiredContextState {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Failing => "failing",
            Self::Pending => "pending",
            Self::Passing => "passing",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct RequiredContextPosture {
    name: String,
    state: RequiredContextState,
    details: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PullRequestThreadList {
    threads: Vec<ReviewThread>,
}

#[derive(Debug, Clone, Deserialize)]
struct PullApproveRequest {
    expected_head_sha: String,
    #[serde(default)]
    body_markdown: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MergeRequest {
    expected_head_sha: String,
    #[serde(default)]
    expected_passport_hash: Option<String>,
    #[serde(default = "default_merge_method")]
    merge_method: String,
    #[serde(default)]
    commit_title: Option<String>,
    #[serde(default)]
    commit_message: Option<String>,
}

fn default_merge_method() -> String {
    "merge".to_string()
}

pub(super) async fn list(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<PullListQuery>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return not_found("load repository pull requests", "repository not found");
    };
    let pulls = match state
        .github
        .core()
        .list_pull_requests(&repo.owner, &repo.name, None)
    {
        Ok(pulls) => pulls,
        Err(error) => return core_error(error, "load repository pull requests"),
    };
    let mut items: Vec<_> = pulls
        .iter()
        .filter(|pr| state_matches(pr, query.state.as_deref()))
        .map(|pr| summary(&state, pr))
        .collect();
    items.sort_by_key(|pr| pr.number);
    Json(PullRequestListResponse {
        total: items.len(),
        items,
    })
    .into_response()
}

pub(super) async fn detail(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
) -> AxumResponse {
    let Some((_, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("load pull request detail", "pull request not found");
    };
    Json(detail_for_pr(&state, &pr, Some(&account.login))).into_response()
}

pub(super) async fn diff(
    State(state): State<Arc<WebState>>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
) -> AxumResponse {
    let Some((_, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("load pull request diff", "pull request not found");
    };
    let files = pr
        .changed_files
        .iter()
        .map(|path| PullRequestDiffFile {
            path: path.clone(),
            old_path: None,
            status: "modified",
            additions: 0,
            deletions: 0,
            risk: None,
            is_binary: false,
            hunks: Vec::new(),
        })
        .collect();
    Json(PullRequestDiff {
        head_sha: pr.head.sha,
        base_sha: pr.base.sha,
        files,
        truncated: false,
    })
    .into_response()
}

pub(super) async fn checks(
    State(state): State<Arc<WebState>>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
) -> AxumResponse {
    let Some((_, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("load pull request checks", "pull request not found");
    };
    Json(checks_for_pr(&state, &pr)).into_response()
}

pub(super) async fn threads(
    State(state): State<Arc<WebState>>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
) -> AxumResponse {
    let Some((_, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("load pull request threads", "pull request not found");
    };
    Json(PullRequestThreadList {
        threads: threads_for_pr(&state, &pr),
    })
    .into_response()
}

/// Authenticated audit history uses the same projection as pull-request detail.
pub(super) async fn review_history(
    State(state): State<Arc<WebState>>,
    Extension(_account): Extension<AccountSummary>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
) -> AxumResponse {
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("load pull request review history", "pull request not found");
    };
    match state
        .github
        .core()
        .list_reviews(&repo.owner, &repo.name, pr.number)
    {
        Ok(reviews) => Json(posture::project_reviews(&pr, reviews)).into_response(),
        Err(error) => core_error(error, "load pull request review history"),
    }
}

pub(super) async fn dismiss_review(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath((id, number, review_id)): AxumPath<(String, u64, String)>,
    body: Bytes,
) -> AxumResponse {
    let request: DismissPullReviewRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return repair_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "pull_dismissal_invalid_request",
                "dismiss pull request review",
                &format!("dismissal body failed validation: {error}"),
                &[
                    "send DismissPullReviewRequest JSON with expected_head_sha and reason",
                    "refresh the review history before withdrawing your current verdict",
                ],
                PROOF_LANE,
                None,
            );
        }
    };
    let review_id = match uuid::Uuid::parse_str(&review_id) {
        Ok(review_id) => review_id,
        Err(_) => {
            return repair_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "pull_dismissal_invalid_target",
                "dismiss pull request review",
                "review target must be a UUID from the pull request review history",
                &["use the id of your current explicit review verdict"],
                PROOF_LANE,
                None,
            );
        }
    };
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("dismiss pull request review", "pull request not found");
    };
    // The auth gate has enforced repository write access. Core validates the
    // exact head and current target under its write lock, without an admin
    // override. Neither the actor nor the target comes from the request body.
    match state.github.core().dismiss_review(
        &repo.owner,
        &repo.name,
        pr.number,
        &account.login,
        jeryu_core::DismissReviewRequest {
            review_id,
            expected_head_sha: request.expected_head_sha,
            reason: request.reason,
        },
    ) {
        Ok(_) => match state
            .github
            .core()
            .get_pull_request(&repo.owner, &repo.name, pr.number)
        {
            Ok(updated) => {
                Json(detail_for_pr(&state, &updated, Some(&account.login))).into_response()
            }
            Err(error) => core_error(error, "reload pull request after review dismissal"),
        },
        Err(error) => core_error(error, "dismiss pull request review"),
    }
}

pub(super) async fn review(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
    body: Bytes,
) -> AxumResponse {
    let request: SubmitReviewRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return repair_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "pull_review_invalid_request",
                "submit pull request review",
                &format!("review body failed validation: {error}"),
                &[
                    "send SubmitReviewRequest JSON with verdict and expected_head_sha",
                    "refresh the PR detail before retrying the review submission",
                ],
                PROOF_LANE,
                None,
            );
        }
    };
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("submit pull request review", "pull request not found");
    };
    if request.expected_head_sha != pr.head.sha {
        return stale_sha(&request.expected_head_sha, &pr.head.sha);
    }
    let comments = request
        .thread_comments
        .into_iter()
        .filter_map(comment_input)
        .collect();
    let event = review_state(request.verdict);
    if event == ReviewState::Approved
        && let Some(response) = self_approval_forbidden(&pr, &account.login)
    {
        return response;
    }
    let review = CreateReviewRequest {
        body: request.body_markdown,
        event,
        comments,
        expected_head_sha: Some(request.expected_head_sha),
    };
    match state.github.core().create_review(
        &repo.owner,
        &repo.name,
        pr.number,
        &account.login,
        review,
    ) {
        Ok(_) => match state
            .github
            .core()
            .get_pull_request(&repo.owner, &repo.name, pr.number)
        {
            Ok(updated) => {
                Json(detail_for_pr(&state, &updated, Some(&account.login))).into_response()
            }
            Err(error) => core_error(error, "reload pull request after review"),
        },
        Err(error) => core_error(error, "submit pull request review"),
    }
}

pub(super) async fn comment(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
    body: Bytes,
) -> AxumResponse {
    let request: CreateReviewCommentRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return repair_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "pull_comment_invalid_request",
                "submit pull request comment",
                &format!("comment body failed validation: {error}"),
                &[
                    "send CreateReviewCommentRequest JSON",
                    "refresh the PR detail before retrying the comment submission",
                ],
                PROOF_LANE,
                None,
            );
        }
    };
    if request.body_markdown.trim().is_empty() {
        return repair_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "pull_comment_body_required",
            "submit pull request comment",
            "comment body_markdown must be non-empty",
            &[
                "enter a review comment body before submitting",
                "retry with the same anchor after refreshing the diff",
            ],
            PROOF_LANE,
            None,
        );
    }
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("submit pull request comment", "pull request not found");
    };
    let comments = comment_input(request).into_iter().collect();
    match state.github.core().create_review(
        &repo.owner,
        &repo.name,
        pr.number,
        &account.login,
        CreateReviewRequest {
            body: None,
            event: ReviewState::Commented,
            comments,
            expected_head_sha: Some(pr.head.sha.clone()),
        },
    ) {
        Ok(_) => Json(PullRequestThreadList {
            threads: threads_for_pr(&state, &pr),
        })
        .into_response(),
        Err(error) => core_error(error, "submit pull request comment"),
    }
}

pub(super) async fn approve(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
    body: Bytes,
) -> AxumResponse {
    let request: PullApproveRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return repair_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "pull_approve_invalid_request",
                "approve pull request",
                &format!("approval body failed validation: {error}"),
                &[
                    "send expected_head_sha from the current PR detail",
                    "refresh the PR detail before approving again",
                ],
                PROOF_LANE,
                None,
            );
        }
    };
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("approve pull request", "pull request not found");
    };
    if request.expected_head_sha != pr.head.sha {
        return stale_sha(&request.expected_head_sha, &pr.head.sha);
    }
    if let Some(response) = self_approval_forbidden(&pr, &account.login) {
        return response;
    }
    match state.github.core().create_review(
        &repo.owner,
        &repo.name,
        pr.number,
        &account.login,
        CreateReviewRequest {
            body: request.body_markdown,
            event: ReviewState::Approved,
            comments: Vec::new(),
            expected_head_sha: Some(request.expected_head_sha),
        },
    ) {
        Ok(_) => match state
            .github
            .core()
            .get_pull_request(&repo.owner, &repo.name, pr.number)
        {
            Ok(updated) => {
                Json(detail_for_pr(&state, &updated, Some(&account.login))).into_response()
            }
            Err(error) => core_error(error, "reload pull request after approval"),
        },
        Err(error) => core_error(error, "approve pull request"),
    }
}

pub(super) async fn merge(
    State(state): State<Arc<WebState>>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
    body: Bytes,
) -> AxumResponse {
    let request: MergeRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return repair_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "pull_merge_invalid_request",
                "merge pull request",
                &format!("merge body failed validation: {error}"),
                &[
                    "send expected_head_sha and expected_passport_hash from the current PR detail",
                    "refresh the PR detail before retrying merge",
                ],
                PROOF_LANE,
                None,
            );
        }
    };
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("merge pull request", "pull request not found");
    };
    if request.expected_head_sha != pr.head.sha {
        return stale_sha(&request.expected_head_sha, &pr.head.sha);
    }
    let current = detail_for_pr(&state, &pr, None);
    if request.expected_passport_hash.as_deref() != current.passport_hash.as_deref() {
        return repair_error(
            StatusCode::CONFLICT,
            "merge_passport_stale",
            "merge pull request",
            "merge passport hash changed since the reviewer loaded the PR",
            &[
                "refresh the PR detail and re-check the merge passport",
                "rerun the mapped proof lane before retrying merge",
            ],
            PROOF_LANE,
            Some(json!({
                "expected_head_sha": request.expected_head_sha,
                "current_head_sha": pr.head.sha,
            })),
        );
    }
    if current.merge_passport.status != MergePassportStatus::Pass {
        return repair_error(
            StatusCode::CONFLICT,
            "merge_blocked",
            "merge pull request",
            "the current exact-head merge passport is blocked",
            &[
                "refresh the PR detail and resolve every merge-passport blocker",
                "rerun the mapped proof lane before retrying merge",
            ],
            PROOF_LANE,
            Some(json!({
                "expected_head_sha": request.expected_head_sha,
                "passport_status": current.merge_passport.status,
                "blockers": current.merge_passport.blockers,
            })),
        );
    }
    let merge_payload = CoreMergePullRequestRequest {
        commit_title: request.commit_title,
        commit_message: request.commit_message,
        sha: Some(request.expected_head_sha),
        merge_method: request.merge_method,
    };
    let merge_body = match serde_json::to_string(&merge_payload) {
        Ok(body) => body,
        Err(error) => {
            return repair_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "pull_request_serialize_failed",
                "merge pull request",
                &format!("could not serialize merge request: {error}"),
                &["retry the merge after refreshing the PR detail"],
                PROOF_LANE,
                Some(json!({ "head_sha": pr.head.sha })),
            );
        }
    };
    let merged = state.github.put(
        &format!(
            "/repos/{}/{}/pulls/{}/merge",
            repo.owner, repo.name, pr.number
        ),
        &merge_body,
    );
    if merged.status != 200 {
        return github_merge_error(merged, &pr);
    }
    match state
        .github
        .core()
        .get_pull_request(&repo.owner, &repo.name, pr.number)
    {
        Ok(updated) => Json(detail_for_pr(&state, &updated, None)).into_response(),
        Err(error) => core_error(error, "reload pull request after merge"),
    }
}

fn resolve_pr(
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

fn self_approval_forbidden(pr: &PullRequest, reviewer: &str) -> Option<AxumResponse> {
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

fn state_matches(pr: &PullRequest, filter: Option<&str>) -> bool {
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

fn detail_for_pr(
    state: &WebState,
    pr: &PullRequest,
    authenticated_login: Option<&str>,
) -> PullRequestDetail {
    let required_contexts = required_contexts(state, pr);
    detail_for_pr_with_required_contexts(state, pr, &required_contexts, authenticated_login)
}

#[cfg(test)]
pub(super) fn detail_for_pr_with_audit_enforcement(
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

fn summary(state: &WebState, pr: &PullRequest) -> PullRequestSummary {
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

mod posture;

#[cfg(test)]
pub(super) use posture::audit_merge_enforced_value;
#[cfg(test)]
use posture::required_contexts_with_enforcement;
use posture::{
    checks_for_pr, comment_input, commit_tree_sha, passport, passport_blockers, passport_hash,
    required_contexts, review_posture, review_state, reviews_for_pr, threads_for_pr, web_pr_state,
};

fn not_found(purpose: &'static str, message: &str) -> AxumResponse {
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

fn stale_sha(expected: &str, current: &str) -> AxumResponse {
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

fn core_error(error: ForgeError, purpose: &'static str) -> AxumResponse {
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

fn github_merge_error(response: crate::Response, pr: &PullRequest) -> AxumResponse {
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

fn repair_error(
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
