//! Repository, README, and document routes for the local web surface.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::response::{Html, IntoResponse, Response as AxumResponse};
use jeryu_core::{
    CheckConclusion, CheckRun, CheckRunStatus, ForgeError, PullRequest, PullRequestState,
    Repository,
};
use jeryu_gitd::refs::RefService;
use jeryu_readmodel::contracts::{
    AvailableAction, BlobEncoding, BlobResponse, EntityHandle, RefKind, RefSelectorItem,
    RenderedMarkdown, RepositoryFacets, RepositoryId, RepositoryListResponse, RepositorySummary,
    RepositoryVisibility, TreeEntry,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::markdown::render_markdown;
use super::{WebState, api_error};

#[derive(Debug, Deserialize)]
struct ReadmeUpdateRequest {
    markdown: String,
}

#[derive(Debug, Serialize)]
struct ReadmeResponse {
    markdown: String,
    #[serde(flatten)]
    rendered_markdown: RenderedMarkdown,
}

pub(super) async fn repos(
    State(state): State<std::sync::Arc<WebState>>,
) -> Json<RepositoryListResponse> {
    Json(repo_list_response(&state))
}

pub(super) async fn repo_detail(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    match find_repo(&state, &id) {
        Some(repo) => Json(repo_summary(&state, &repo)).into_response(),
        None => api_error_with_hint(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
            ApiErrorHint {
                purpose: "load repository metadata",
                reason: "not_found",
                common_fixes: &[
                    "verify the repository id or owner/name pair",
                    "refresh the local forge import before retrying",
                ],
                docs_url: "docs/errors.md#not-found",
                repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40",
            },
        ),
    }
}

pub(super) async fn repo_refs(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        );
    };
    let default_branch = repo.default_branch.clone();
    Json(vec![RefSelectorItem {
        name: default_branch.clone(),
        sha: "unknown".to_string(),
        kind: RefKind::Branch,
        protected: state
            .github
            .core()
            .get_branch_protection(&repo.owner, &repo.name, &default_branch)
            .is_ok(),
    }])
    .into_response()
}

pub(super) async fn repo_tree(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    if find_repo(&state, &id).is_none() {
        return api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        );
    }
    Json(Vec::<TreeEntry>::new()).into_response()
}

pub(super) async fn repo_blob(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        );
    };
    let readme = readme_markdown(&state, &repo);
    let rendered = render_markdown(&readme);
    Json(BlobResponse {
        repo: repo_id(&repo),
        path: "README.md".to_string(),
        ref_name: repo.default_branch,
        sha: "unknown".to_string(),
        size_bytes: readme.len() as u64,
        mime: "text/markdown".to_string(),
        encoding: BlobEncoding::Utf8,
        text: Some(readme),
        base64: None,
        rendered_markdown: Some(rendered),
        is_binary: false,
    })
    .into_response()
}

pub(super) async fn repo_raw(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        );
    };
    Html(readme_markdown(&state, &repo)).into_response()
}

pub(super) async fn repo_readme(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return readme_not_found_error();
    };
    Json(readme_response(&state, &repo)).into_response()
}

pub(super) async fn repo_readme_update(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return readme_not_found_error();
    };
    let request: ReadmeUpdateRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return api_error_with_hint(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_input",
                "readme update body failed validation",
                ApiErrorHint {
                    purpose: "update repository README",
                    reason: "invalid_input",
                    common_fixes: &[
                        "send JSON with a markdown string field",
                        "regenerate the managed README block from the fresh Jankurai artifact",
                    ],
                    docs_url: "docs/release-process.md#required-local-gates",
                    repair_hint: &format!(
                        "rerun bash ops/ci/publish-readme-score.sh --verify (body parse error: {error})"
                    ),
                },
            );
        }
    };
    match state
        .github
        .core()
        .set_repository_readme(&repo.owner, &repo.name, request.markdown)
    {
        Ok(markdown) => {
            Json(readme_response_with_markdown(&state, &repo, markdown)).into_response()
        }
        Err(ForgeError::NotFound(_)) => readme_not_found_error(),
        Err(ForgeError::Storage(err)) => api_error_with_hint(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            "repository README could not be persisted",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "storage_failed",
                common_fixes: &[
                    "check the SQLite database path and write permissions",
                    "reopen the local forge store and rerun the publish helper",
                ],
                docs_url: "docs/release-process.md#required-local-gates",
                repair_hint: &format!(
                    "rerun bash ops/ci/publish-readme-score.sh --verify (storage error: {err})"
                ),
            },
        ),
        Err(ForgeError::Conflict(err)) => api_error_with_hint(
            axum::http::StatusCode::CONFLICT,
            "conflict",
            "repository README update conflicted",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "conflict",
                common_fixes: &[
                    "refresh the local repo state before retrying the publish helper",
                    "replay the update against the latest README content",
                ],
                docs_url: "docs/release-process.md#required-local-gates",
                repair_hint: &format!(
                    "rerun bash ops/ci/publish-readme-score.sh --verify (conflict: {err})"
                ),
            },
        ),
        Err(ForgeError::Validation(err)) => api_error_with_hint(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "repository README update failed validation",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "invalid_input",
                common_fixes: &[
                    "send a JSON body with a markdown string field",
                    "regenerate the managed score block from target/jankurai/repo-score.json",
                ],
                docs_url: "docs/release-process.md#required-local-gates",
                repair_hint: &format!(
                    "rerun bash ops/ci/publish-readme-score.sh --verify (validation error: {err})"
                ),
            },
        ),
        Err(ForgeError::BranchProtection(err)) => api_error_with_hint(
            axum::http::StatusCode::FORBIDDEN,
            "policy_denied",
            "repository README update was blocked by policy",
            ApiErrorHint {
                purpose: "persist repository README",
                reason: "policy_denied",
                common_fixes: &[
                    "inspect the repository policy reason instead of bypassing the guard",
                    "supply the required proof or trust evidence",
                ],
                docs_url: "docs/errors.md#policy-denied",
                repair_hint: &format!(
                    "rerun bash ops/ci/publish-readme-score.sh --verify (policy error: {err})"
                ),
            },
        ),
    }
}

pub(super) fn repo_list_response(state: &WebState) -> RepositoryListResponse {
    let repositories = repo_summaries(state);
    let mut owners = BTreeSet::new();
    for repo in &repositories {
        owners.insert(repo.id.owner.clone());
    }
    RepositoryListResponse {
        generated_at: state.tui.generated_at.to_rfc3339(),
        total: repositories.len() as u64,
        repositories,
        facets: RepositoryFacets {
            hosts: vec!["jeryu".to_string()],
            owners: owners.into_iter().collect(),
            families: Vec::new(),
            languages: Vec::new(),
        },
    }
}

pub(super) fn repo_summaries(state: &WebState) -> Vec<RepositorySummary> {
    state
        .github
        .core()
        .list_repositories(None)
        .into_iter()
        .map(|repo| repo_summary(state, &repo))
        .collect()
}

pub(super) fn repo_summary(state: &WebState, repo: &Repository) -> RepositorySummary {
    let pulls = state
        .github
        .core()
        .list_pull_requests(&repo.owner, &repo.name, None)
        .unwrap_or_default();
    let checks = state
        .github
        .core()
        .list_check_runs(&repo.owner, &repo.name, None)
        .map(|runs| runs.check_runs)
        .unwrap_or_default();
    let current = current_check_runs(state, repo, &pulls, &checks);
    let failing_checks = current
        .iter()
        .filter(|check| {
            check.conclusion == Some(CheckConclusion::Failure) && check.name != GITHUB_MIRROR_CHECK
        })
        .count() as u32;
    let running_jobs = current
        .iter()
        .filter(|check| check.status == CheckRunStatus::InProgress)
        .count() as u32;
    RepositorySummary {
        id: repo_id(repo),
        entity: EntityHandle {
            kind: "repo".to_string(),
            id: repo.id.to_string(),
        },
        description: repo.description.clone(),
        visibility: if repo.private {
            RepositoryVisibility::Private
        } else {
            RepositoryVisibility::Public
        },
        default_branch: repo.default_branch.clone(),
        family: None,
        topics: Vec::new(),
        language: None,
        health: if failing_checks > 0 {
            "warning".to_string()
        } else {
            "healthy".to_string()
        },
        open_pull_requests: pulls.iter().filter(|pr| pull_is_open(pr)).count() as u32,
        failing_checks,
        running_jobs,
        active_agents: 0,
        blocked_agents: 0,
        updated_at: repo.updated_at.to_rfc3339(),
        clone_http_url: Some(format!("/repos/{}.git", repo.full_name)),
        clone_ssh_url: None,
        available_actions: vec![AvailableAction {
            action_id: "repo.open".to_string(),
            label: "Open".to_string(),
            risk: None,
        }],
    }
}

/// Push-mirror bookkeeping check. A mirror hiccup is surfaced through the
/// dedicated mirror-status field, not as repository ill-health.
const GITHUB_MIRROR_CHECK: &str = "jeryu/github-mirror";

fn pull_is_open(pr: &PullRequest) -> bool {
    !matches!(
        pr.state,
        PullRequestState::Closed | PullRequestState::Merged
    )
}

/// Check runs that describe the repository's CURRENT state: the latest run per
/// `(head_sha, name)` across the open pull-request heads plus the
/// default-branch head.
///
/// The check-run store is append-only history. Counting it wholesale
/// resurfaces every legacy failure forever (jeryu/jeryu carried ~2k stale
/// failures from retired seeding), so health and the list badges must only
/// see the newest verdict per check name on a sha that is still live.
fn current_check_runs<'a>(
    state: &WebState,
    repo: &Repository,
    pulls: &[PullRequest],
    checks: &'a [CheckRun],
) -> Vec<&'a CheckRun> {
    let default_head = default_branch_head(state, repo);
    let mut relevant: BTreeSet<&str> = pulls
        .iter()
        .filter(|pr| pull_is_open(pr))
        .map(|pr| pr.head.sha.as_str())
        .collect();
    if let Some(sha) = default_head.as_deref() {
        relevant.insert(sha);
    }
    let mut latest: BTreeMap<(&str, &str), &CheckRun> = BTreeMap::new();
    for check in checks {
        if !relevant.contains(check.head_sha.as_str()) {
            continue;
        }
        match latest.entry((check.head_sha.as_str(), check.name.as_str())) {
            Entry::Vacant(slot) => {
                slot.insert(check);
            }
            Entry::Occupied(mut slot) => {
                let held = *slot.get();
                // `>=` so runs with identical timestamps resolve to the
                // later-listed one — the store appends in creation order.
                if (check.started_at, check.completed_at) >= (held.started_at, held.completed_at) {
                    slot.insert(check);
                }
            }
        }
    }
    latest.into_values().collect()
}

/// Resolve the default-branch head commit, tolerating repos with no bare
/// storage (metadata-only imports) — those simply contribute no sha.
fn default_branch_head(state: &WebState, repo: &Repository) -> Option<String> {
    let bare = state
        .repo_manager
        .open_parts(&repo.owner, &repo.name)
        .ok()?;
    RefService::new((*state.repo_manager).clone())
        .resolve_commit(&bare, &format!("refs/heads/{}", repo.default_branch))
        .ok()
        .flatten()
}

pub(super) fn repo_id(repo: &Repository) -> RepositoryId {
    RepositoryId {
        id: repo.id.to_string(),
        host: "jeryu".to_string(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
    }
}

pub(super) fn find_repo(state: &WebState, id: &str) -> Option<Repository> {
    state
        .github
        .core()
        .list_repositories(None)
        .into_iter()
        .find(|repo| repo.id.to_string() == id || repo.full_name == id)
}

fn readme_markdown(state: &WebState, repo: &Repository) -> String {
    state
        .github
        .core()
        .readme_or_default(&repo.owner, &repo.name, default_readme_markdown(repo))
        .unwrap_or_else(|_| default_readme_markdown(repo))
}

fn readme_response(state: &WebState, repo: &Repository) -> ReadmeResponse {
    let markdown = readme_markdown(state, repo);
    readme_response_with_markdown(state, repo, markdown)
}

fn readme_response_with_markdown(
    _state: &WebState,
    _repo: &Repository,
    markdown: String,
) -> ReadmeResponse {
    ReadmeResponse {
        rendered_markdown: render_markdown(&markdown),
        markdown,
    }
}

fn default_readme_markdown(repo: &Repository) -> String {
    format!(
        "# {}\n\nRepository metadata is live. Source import has not attached a README yet.\n",
        repo.full_name
    )
}

fn readme_not_found_error() -> AxumResponse {
    api_error_with_hint(
        axum::http::StatusCode::NOT_FOUND,
        "not_found",
        "repository not found",
        ApiErrorHint {
            purpose: "load repository README",
            reason: "not_found",
            common_fixes: &[
                "verify the repository id or owner/name pair",
                "refresh the local forge import before retrying",
            ],
            docs_url: "docs/errors.md#not-found",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40",
        },
    )
}

struct ApiErrorHint<'a> {
    purpose: &'a str,
    reason: &'a str,
    common_fixes: &'a [&'a str],
    docs_url: &'a str,
    repair_hint: &'a str,
}

fn api_error_with_hint(
    status: axum::http::StatusCode,
    code: &str,
    message: &str,
    hint: ApiErrorHint<'_>,
) -> AxumResponse {
    (
        status,
        Json(json!({
            "code": code,
            "message": message,
            "jeryu_repair_hint": {
                "purpose": hint.purpose,
                "reason": hint.reason,
                "common_fixes": hint.common_fixes,
                "docs_url": hint.docs_url,
                "repair_hint": hint.repair_hint,
            }
        })),
    )
        .into_response()
}
