//! PR cockpit BFF routes for the SPA's W-FE-11 surface.
//!
//! These routes translate the local forge's authoritative pull request,
//! review, and check-run state into the typed web contracts consumed by the
//! React cockpit. Missing diff hunks or review threads are explicit empty
//! payloads derived from the PR metadata, never synthetic review content.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::{
    CheckConclusion, CheckRun, CheckRunStatus, CreateReviewRequest, ForgeError,
    MergePullRequestRequest as CoreMergePullRequestRequest, PullRequest, ReviewCommentInput,
    ReviewState,
};
use jeryu_readmodel::contracts::{
    AgentPosture, AvailableAction, CheckPosture, CreateReviewCommentRequest, EntityHandle,
    MergePassport, MergePassportBlocker, MergePassportStatus, Mergeability, PullRequestDetail,
    PullRequestState as WebPullRequestState, PullRequestSummary, ReviewComment as WebReviewComment,
    ReviewPosture, ReviewThread, ReviewVerdict, SubmitReviewRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
    AxumPath((id, number)): AxumPath<(String, u64)>,
) -> AxumResponse {
    let Some((_, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("load pull request detail", "pull request not found");
    };
    Json(detail_for_pr(&state, &pr)).into_response()
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

pub(super) async fn review(
    State(state): State<Arc<WebState>>,
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
    let review = CreateReviewRequest {
        body: request.body_markdown,
        event: review_state(request.verdict),
        comments,
    };
    match state.github.core().create_review(
        &repo.owner,
        &repo.name,
        pr.number,
        "local-reviewer",
        review,
    ) {
        Ok(_) => match state
            .github
            .core()
            .get_pull_request(&repo.owner, &repo.name, pr.number)
        {
            Ok(updated) => Json(detail_for_pr(&state, &updated)).into_response(),
            Err(error) => core_error(error, "reload pull request after review"),
        },
        Err(error) => core_error(error, "submit pull request review"),
    }
}

pub(super) async fn comment(
    State(state): State<Arc<WebState>>,
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
        "local-reviewer",
        CreateReviewRequest {
            body: None,
            event: ReviewState::Commented,
            comments,
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
    match state.github.core().create_review(
        &repo.owner,
        &repo.name,
        pr.number,
        "local-reviewer",
        CreateReviewRequest {
            body: request.body_markdown,
            event: ReviewState::Approved,
            comments: Vec::new(),
        },
    ) {
        Ok(_) => match state
            .github
            .core()
            .get_pull_request(&repo.owner, &repo.name, pr.number)
        {
            Ok(updated) => Json(detail_for_pr(&state, &updated)).into_response(),
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
    let current = detail_for_pr(&state, &pr);
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
    let merge_payload = CoreMergePullRequestRequest {
        commit_title: request.commit_title,
        commit_message: request.commit_message,
        sha: Some(request.expected_head_sha),
        merge_method: request.merge_method,
    };
    match state
        .github
        .core()
        .merge_pull_request(&repo.owner, &repo.name, pr.number, merge_payload)
    {
        Ok(_) => match state
            .github
            .core()
            .get_pull_request(&repo.owner, &repo.name, pr.number)
        {
            Ok(updated) => Json(detail_for_pr(&state, &updated)).into_response(),
            Err(error) => core_error(error, "reload pull request after merge"),
        },
        Err(ForgeError::BranchProtection(reason)) => repair_error(
            StatusCode::CONFLICT,
            "merge_blocked",
            "merge pull request",
            &reason,
            &[
                "inspect the merge passport blockers before retrying",
                "rerun required checks and collect approvals for the current head",
            ],
            PROOF_LANE,
            Some(json!({ "head_sha": pr.head.sha })),
        ),
        Err(error) => core_error(error, "merge pull request"),
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

fn detail_for_pr(state: &WebState, pr: &PullRequest) -> PullRequestDetail {
    let summary = summary(state, pr);
    let merge_passport = passport(&summary, pr);
    PullRequestDetail {
        passport_hash: summary.passport_hash.clone(),
        summary,
        description: pr.body.clone(),
        merge_passport,
    }
}

fn summary(state: &WebState, pr: &PullRequest) -> PullRequestSummary {
    let repo = find_repo(state, &format!("{}/{}", pr.owner, pr.repo))
        .expect("PR owner/repo must resolve to a repository");
    let checks = checks_for_pr(state, pr);
    let review = review_posture(state, pr);
    let web_state = web_pr_state(pr);
    let mergeable = !pr.draft
        && !pr.merged
        && matches!(web_state, WebPullRequestState::Open)
        && pr.mergeable
        && checks.failing == 0
        && checks.pending == 0
        && checks.total > 0
        && review.approvals >= review.required_approvals
        && review.unresolved_threads == 0;
    let reason = if mergeable {
        None
    } else if pr.draft {
        Some("draft pull request".to_string())
    } else if checks.total == 0 {
        Some("no head checks recorded".to_string())
    } else if checks.failing > 0 {
        Some("failing checks".to_string())
    } else if checks.pending > 0 {
        Some("queued or running checks".to_string())
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
    let blocker_count = passport_blockers(
        &CheckPosture {
            total: checks.total,
            passing: checks.passing,
            failing: checks.failing,
            pending: checks.pending,
            skipped: checks.skipped,
        },
        &review,
        pr,
    )
    .len();
    let passport_hash = format!(
        "passport:{}:{:?}:{}:{}:{}:{}",
        pr.head.sha, status, blocker_count, checks.failing, checks.pending, review.approvals
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

fn passport(summary: &PullRequestSummary, pr: &PullRequest) -> MergePassport {
    let status = if summary.mergeable.can_merge {
        MergePassportStatus::Pass
    } else {
        MergePassportStatus::Blocked
    };
    MergePassport {
        status,
        head_sha: summary.head_sha.clone(),
        blockers: passport_blockers(&summary.checks, &summary.review, pr),
        evaluated_at: server_time(),
    }
}

fn passport_blockers(
    checks: &CheckPosture,
    review: &ReviewPosture,
    pr: &PullRequest,
) -> Vec<MergePassportBlocker> {
    let mut blockers = Vec::new();
    if pr.draft {
        blockers.push(blocker(
            "passport_blocked_draft",
            "Draft pull requests cannot be merged.",
            None,
        ));
    }
    if checks.total == 0 {
        blockers.push(blocker(
            "passport_blocked_checks_missing",
            "No checks have run on this head.",
            Some("Create or refresh check-runs for the PR head before merge."),
        ));
    }
    if checks.failing > 0 {
        blockers.push(blocker(
            "passport_blocked_checks",
            "One or more checks are failing.",
            None,
        ));
    }
    if checks.pending > 0 {
        blockers.push(blocker(
            "passport_blocked_pending_checks",
            "Checks are still queued or running.",
            None,
        ));
    }
    if review.approvals < review.required_approvals {
        blockers.push(blocker(
            "passport_blocked_approvals",
            "Required approver count not satisfied.",
            None,
        ));
    }
    if review.unresolved_threads > 0 {
        blockers.push(blocker(
            "passport_blocked_threads",
            "Review threads are unresolved.",
            None,
        ));
    }
    if !pr.mergeable && pr.mergeable_state != "clean" && blockers.is_empty() {
        blockers.push(blocker(
            "passport_blocked_mergeability",
            "Forge mergeability is not clean.",
            Some(&pr.mergeable_state),
        ));
    }
    blockers
}

fn blocker(code: &str, message: &str, details: Option<&str>) -> MergePassportBlocker {
    MergePassportBlocker {
        code: code.to_string(),
        message: message.to_string(),
        details: details.map(ToString::to_string),
    }
}

fn checks_for_pr(state: &WebState, pr: &PullRequest) -> PullRequestChecks {
    let runs = state
        .github
        .core()
        .list_check_runs(&pr.owner, &pr.repo, Some(&pr.head.sha))
        .map(|list| list.check_runs)
        .unwrap_or_default();
    let mut passing = 0;
    let mut failing = 0;
    let mut pending = 0;
    let mut skipped = 0;
    let checks = runs
        .iter()
        .map(|run| {
            match check_bucket(run) {
                "success" => passing += 1,
                "failure" => failing += 1,
                "pending" => pending += 1,
                "skipped" => skipped += 1,
                _ => {}
            }
            PullRequestCheck {
                id: run.id.to_string(),
                name: run.name.clone(),
                status: check_status(run).to_string(),
                conclusion: run.conclusion.as_ref().map(conclusion),
                details_url: run.details_url.clone(),
                description: run.output.as_ref().map(|output| output.summary.clone()),
                started_at: Some(run.started_at.to_rfc3339()),
                completed_at: run.completed_at.map(|at| at.to_rfc3339()),
            }
        })
        .collect();
    PullRequestChecks {
        total: runs.len() as u32,
        passing,
        failing,
        pending,
        skipped,
        checks,
    }
}

fn check_bucket(run: &CheckRun) -> &'static str {
    match run.status {
        CheckRunStatus::Queued | CheckRunStatus::InProgress => "pending",
        CheckRunStatus::Completed => match run.conclusion {
            Some(CheckConclusion::Success) => "success",
            Some(CheckConclusion::Skipped | CheckConclusion::Neutral) => "skipped",
            _ => "failure",
        },
    }
}

fn check_status(run: &CheckRun) -> &'static str {
    match run.status {
        CheckRunStatus::Queued => "queued",
        CheckRunStatus::InProgress => "running",
        CheckRunStatus::Completed => match run.conclusion {
            Some(CheckConclusion::Success) => "success",
            Some(CheckConclusion::Skipped) => "skipped",
            Some(CheckConclusion::Cancelled) => "cancelled",
            Some(CheckConclusion::Neutral) => "neutral",
            _ => "failure",
        },
    }
}

fn conclusion(value: &CheckConclusion) -> String {
    match value {
        CheckConclusion::ActionRequired => "action_required",
        CheckConclusion::Cancelled => "cancelled",
        CheckConclusion::Failure => "failure",
        CheckConclusion::Neutral => "neutral",
        CheckConclusion::Success => "success",
        CheckConclusion::Skipped => "skipped",
        CheckConclusion::Superseded => "stale",
        CheckConclusion::TimedOut => "timed_out",
    }
    .to_string()
}

fn review_posture(state: &WebState, pr: &PullRequest) -> ReviewPosture {
    let reviews = state
        .github
        .core()
        .list_reviews(&pr.owner, &pr.repo, pr.number)
        .unwrap_or_default();
    let comments = state
        .github
        .core()
        .list_review_comments(&pr.owner, &pr.repo, pr.number)
        .unwrap_or_default();
    ReviewPosture {
        required_approvals: 1,
        approvals: reviews
            .iter()
            .filter(|review| review.state == ReviewState::Approved)
            .count() as u32,
        changes_requested: reviews
            .iter()
            .filter(|review| review.state == ReviewState::ChangesRequested)
            .count() as u32,
        unresolved_threads: comments.len() as u32,
        user_review_state: None,
    }
}

fn threads_for_pr(state: &WebState, pr: &PullRequest) -> Vec<ReviewThread> {
    let repo = find_repo(state, &format!("{}/{}", pr.owner, pr.repo))
        .expect("PR owner/repo must resolve to a repository");
    state
        .github
        .core()
        .list_review_comments(&pr.owner, &pr.repo, pr.number)
        .unwrap_or_default()
        .into_iter()
        .map(|comment| ReviewThread {
            id: comment.id.to_string(),
            repo: repo_id(&repo),
            pr_number: pr.number as u32,
            resolved: false,
            file_path: Some(comment.path.clone()),
            line: comment.line.map(|line| line as u32),
            anchor_sha: Some(pr.head.sha.clone()),
            comments: vec![WebReviewComment {
                id: comment.id.to_string(),
                author: comment.author,
                body_markdown: comment.body,
                body_html: None,
                created_at: comment.created_at.to_rfc3339(),
                edited_at: None,
                suggestion: None,
                evidence: None,
            }],
            created_at: comment.created_at.to_rfc3339(),
            updated_at: comment.created_at.to_rfc3339(),
        })
        .collect()
}

fn comment_input(request: CreateReviewCommentRequest) -> Option<ReviewCommentInput> {
    let path = request.file_path?;
    Some(ReviewCommentInput {
        path,
        line: request.line.map(u64::from),
        body: request.body_markdown,
    })
}

fn review_state(verdict: ReviewVerdict) -> ReviewState {
    match verdict {
        ReviewVerdict::Comment => ReviewState::Commented,
        ReviewVerdict::Approve => ReviewState::Approved,
        ReviewVerdict::RequestChanges => ReviewState::ChangesRequested,
    }
}

fn web_pr_state(pr: &PullRequest) -> WebPullRequestState {
    if pr.merged || matches!(pr.state, jeryu_core::PullRequestState::Merged) {
        WebPullRequestState::Merged
    } else if matches!(pr.state, jeryu_core::PullRequestState::Closed) {
        WebPullRequestState::Closed
    } else {
        WebPullRequestState::Open
    }
}

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
        "details": details.clone().unwrap_or_else(|| json!({})),
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
