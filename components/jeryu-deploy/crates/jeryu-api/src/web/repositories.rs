//! Repository, README, and document routes for the local web surface.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use axum::Json;
use axum::body::Bytes;
use axum::extract::Extension;
use axum::extract::Query;
use axum::extract::{Path as AxumPath, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response as AxumResponse};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use jeryu_core::{
    AccountSummary, CheckConclusion, CheckRun, CheckRunStatus, ForgeError, PullRequest,
    PullRequestState, RecordJankuraiScoreRequest, Repository, UserRole,
};
use jeryu_gitd::refs::RefService;
use jeryu_readmodel::contracts::{
    AvailableAction, BlobEncoding, BlobResponse, EntityHandle, JankuraiScoreListResponse,
    JankuraiScoreSummary, RefKind, RefSelectorItem, RenderedMarkdown, RepositoryFacets,
    RepositoryId, RepositoryListResponse, RepositoryMirrorStatus, RepositorySummary,
    RepositoryVisibility, ToolFleetEntry, ToolFleetResponse, TreeEntry, TreeEntryKind,
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

#[derive(Debug, Default, Deserialize)]
pub(super) struct SourceQuery {
    #[serde(rename = "ref")]
    pub(super) ref_name: Option<String>,
    pub(super) path: Option<String>,
    pub(super) render: Option<String>,
}

type SourceResult<T> = Result<T, Box<AxumResponse>>;
const MAX_BLOB_PREVIEW_BYTES: u64 = 1024 * 1024;
const MAX_README_PREVIEW_BYTES: u64 = 512 * 1024;

/// Query parameters of `GET /api/v1/repos` — the SPA sends all of these
/// (`apps/web/src/hooks/useRepositories.ts` builds the URL), so every one
/// must filter server-side; the family drill-down page in particular is
/// nothing but `?family=`.
#[derive(Debug, Default, Deserialize)]
pub(super) struct RepoListQuery {
    pub(super) q: Option<String>,
    pub(super) host: Option<String>,
    pub(super) visibility: Option<String>,
    pub(super) family: Option<String>,
    pub(super) archived: Option<String>,
    pub(super) sort: Option<String>,
}

pub(super) async fn repos(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    Query(query): Query<RepoListQuery>,
) -> Json<RepositoryListResponse> {
    Json(filtered_repo_list_response_for_user(
        &state,
        &query,
        Some(&account),
    ))
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

/// PATCH /api/v1/repos/:id — update mutable repository metadata.
///
/// Body is a JSON object; only the keys that are PRESENT are applied, so
/// `{"family": "veox-split"}` sets the family, `{"family": null}` clears it,
/// and an absent key leaves it untouched. Hand-parsed because serde's
/// `Option<Option<T>>` cannot distinguish absent from null.
pub(super) async fn repo_update(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        );
    };
    let parsed: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            return repo_update_invalid(&format!("body is not valid JSON: {error}"));
        }
    };
    let Some(fields) = parsed.as_object() else {
        return repo_update_invalid("body must be a JSON object");
    };
    if let Some(unknown) = fields.keys().find(|key| key.as_str() != "family") {
        return repo_update_invalid(&format!("unknown field: {unknown}"));
    }
    let mut updated = repo.clone();
    if let Some(family_value) = fields.get("family") {
        let family = match family_value {
            serde_json::Value::Null => None,
            serde_json::Value::String(name) => Some(name.clone()),
            _ => return repo_update_invalid("family must be a string or null"),
        };
        updated = match state
            .github
            .core()
            .set_repository_family(&repo.owner, &repo.name, family)
        {
            Ok(repo) => repo,
            Err(ForgeError::Validation(reason)) => {
                return repo_update_invalid(&reason);
            }
            Err(ForgeError::NotFound(_)) => {
                return api_error(
                    axum::http::StatusCode::NOT_FOUND,
                    "not_found",
                    "repository not found",
                );
            }
            Err(error) => {
                return api_error(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_failed",
                    &format!("repository update could not be persisted: {error}"),
                );
            }
        };
    }
    Json(repo_summary(&state, &updated)).into_response()
}

fn repo_update_invalid(reason: &str) -> AxumResponse {
    api_error_with_hint(
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_input",
        "repository update body failed validation",
        ApiErrorHint {
            purpose: "update repository metadata",
            reason: "invalid_input",
            common_fixes: &[
                "send a JSON object with a family string field",
                "send {\"family\": null} to clear the grouping",
            ],
            docs_url: "docs/errors.md#invalid-input",
            repair_hint: &format!("fix the PATCH body and retry ({reason})"),
        },
    )
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ScoreListQuery {
    pub(super) branch: Option<String>,
    pub(super) sha: Option<String>,
}

/// GET /api/v1/repos/:id/jankurai-scores[?branch=&sha=] — ingested audit
/// outcomes, newest first. The backfill sweep uses `?sha=` as its
/// idempotency probe.
pub(super) async fn repo_jankurai_scores_list(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<ScoreListQuery>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        );
    };
    let scores = state
        .github
        .core()
        .list_jankurai_scores(
            &repo.owner,
            &repo.name,
            query.branch.as_deref(),
            query.sha.as_deref(),
        )
        .unwrap_or_default();
    Json(JankuraiScoreListResponse {
        scores: scores.iter().map(score_summary).collect(),
    })
    .into_response()
}

/// GET /api/v1/fleet/tool-adoption — the tool-compounding visibility matrix.
/// Projects every repo's latest recorded score (`tool_adoption.items`) into a
/// per-tool view of who adopts each tool and who is applicable-but-missing. No
/// new data is computed — it reuses the scores the forge already ingests.
pub(super) async fn fleet_tool_adoption(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
) -> AxumResponse {
    let core = state.github.core();
    // tool id -> (category, adopting repos, applicable-but-missing repos)
    let mut by_tool: BTreeMap<String, (String, BTreeSet<String>, BTreeSet<String>)> =
        BTreeMap::new();
    let mut repos_scored: u32 = 0;

    for repo in core.list_repositories(None).into_iter().filter(|repo| {
        account.role == UserRole::Admin
            || core.user_can_read_repo(&account.login, &repo.owner, &repo.name)
    }) {
        let slug = format!("{}/{}", repo.owner, repo.name);
        // Newest recorded score for the repo, across any branch.
        let scores = core
            .list_jankurai_scores(&repo.owner, &repo.name, None, None)
            .unwrap_or_default();
        let Some(report) = scores
            .first()
            .and_then(|score| score.report_json.as_deref())
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        else {
            continue;
        };
        let Some(items) = report
            .get("tool_adoption")
            .and_then(|ta| ta.get("items"))
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        repos_scored += 1;
        for item in items {
            let Some(id) = item.get("id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if id.is_empty() {
                continue;
            }
            let category = item
                .get("category")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let applicable = item
                .get("applicable")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let status = item
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let entry = by_tool
                .entry(id.to_string())
                .or_insert_with(|| (category.to_string(), BTreeSet::new(), BTreeSet::new()));
            if entry.0.is_empty() {
                entry.0 = category.to_string();
            }
            // "adopted" = configured or better; anything else applicable is a
            // "should adopt" opportunity. Not-applicable tools are ignored.
            let adopted = !matches!(status, "" | "missing" | "not_applicable" | "not_configured");
            if applicable && adopted {
                entry.1.insert(slug.clone());
            } else if applicable {
                entry.2.insert(slug.clone());
            }
        }
    }

    let tools = by_tool
        .into_iter()
        .map(|(tool, (category, adopting, missing))| ToolFleetEntry {
            tool,
            category,
            adopting_repos: adopting.into_iter().collect(),
            applicable_missing_repos: missing.into_iter().collect(),
        })
        .collect();
    Json(ToolFleetResponse {
        repos_scored,
        tools,
    })
    .into_response()
}

/// POST /api/v1/repos/:id/jankurai-scores — global-admin maintenance ingest for
/// a server-side CI lane or backfill sweep. Idempotent per (branch, commit_sha).
/// Ordinary repository writers cannot mint audit evidence.
pub(super) async fn repo_jankurai_scores_ingest(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    if !super::auth::can_publish_external_ci_evidence(&account) {
        return api_error(
            axum::http::StatusCode::FORBIDDEN,
            "permission_denied",
            "jankurai score publication requires global-admin access",
        );
    }
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        );
    };
    let request: RecordJankuraiScoreRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return score_ingest_invalid(&format!("body failed to parse: {error}"));
        }
    };
    match state
        .github
        .core()
        .record_jankurai_score(&repo.owner, &repo.name, request)
    {
        Ok(score) => (axum::http::StatusCode::CREATED, Json(score_summary(&score))).into_response(),
        Err(ForgeError::Validation(reason)) => score_ingest_invalid(&reason),
        Err(ForgeError::NotFound(_)) => api_error(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "repository not found",
        ),
        Err(error) => api_error(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "storage_failed",
            &format!("score could not be persisted: {error}"),
        ),
    }
}

fn score_summary(score: &jeryu_core::JankuraiScore) -> JankuraiScoreSummary {
    JankuraiScoreSummary {
        branch: score.branch.clone(),
        commit_sha: score.commit_sha.clone(),
        score: score.score,
        hard_findings: score.hard_findings,
        decision: score.decision.clone(),
        caps_applied: score.caps_applied.clone(),
        created_at: score.created_at.to_rfc3339(),
    }
}

fn score_ingest_invalid(reason: &str) -> AxumResponse {
    api_error_with_hint(
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_input",
        "jankurai score submission failed validation",
        ApiErrorHint {
            purpose: "ingest a jankurai audit score",
            reason: "invalid_input",
            common_fixes: &[
                "send JSON with branch, commit_sha, and decision fields",
                "score must be 0-100 or null for tool-failed audits",
            ],
            docs_url: "docs/errors.md#invalid-input",
            repair_hint: &format!("fix the POST body and retry ({reason})"),
        },
    )
}

mod source;

pub(super) use source::{
    repo_blob, repo_raw, repo_readme, repo_readme_update, repo_refs, repo_tree,
};

/// Unfiltered listing — test convenience over the filtered handler path.
#[cfg(test)]
pub(super) fn repo_list_response(state: &WebState) -> RepositoryListResponse {
    filtered_repo_list_response(state, &RepoListQuery::default())
}

/// Build the repos listing with the SPA's filters applied server-side.
///
/// Facets are computed from the UNFILTERED listing so the filter chips stay
/// populated while a filter narrows the repositories array. `archived`
/// matches the SPA contract: absent → active repos only, `1`/`true` → only
/// archived ones.
#[cfg(test)]
pub(super) fn filtered_repo_list_response(
    state: &WebState,
    query: &RepoListQuery,
) -> RepositoryListResponse {
    filtered_repo_list_response_for_user(state, query, None)
}

pub(super) fn filtered_repo_list_response_for_user(
    state: &WebState,
    query: &RepoListQuery,
    account: Option<&AccountSummary>,
) -> RepositoryListResponse {
    let all_unfiltered = state.github.core().list_repositories(None);
    let all: Vec<_> = all_unfiltered
        .into_iter()
        .filter(|repo| {
            account.is_none_or(|account| {
                account.role == UserRole::Admin
                    || state.github.core().user_can_read_repo(
                        &account.login,
                        &repo.owner,
                        &repo.name,
                    )
            })
        })
        .collect();
    let mut owners = BTreeSet::new();
    let mut families = BTreeSet::new();
    for repo in &all {
        owners.insert(repo.owner.clone());
        if let Some(family) = effective_family(state, repo) {
            families.insert(family);
        }
    }

    let archived_only = matches!(
        query.archived.as_deref(),
        Some("1") | Some("true") | Some("yes")
    );
    let needle = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_lowercase);
    // Filter on the core repos BEFORE summarizing: summaries resolve the
    // default-branch head per repo, so narrowing first keeps a filtered
    // request from paying for the whole registry.
    let mut repositories: Vec<RepositorySummary> = all
        .into_iter()
        .filter(|repo| repo.archived == archived_only)
        // Every registry repo lives on the local forge host.
        .filter(|_| query.host.as_deref().is_none_or(|host| host == "jeryu"))
        .filter(|repo| {
            query.visibility.as_deref().is_none_or(|visibility| {
                let actual = if repo.private { "private" } else { "public" };
                actual == visibility
            })
        })
        .filter(|repo| {
            // Match on the same effective family the summaries expose, so the
            // split-catalog drill-down lists its members even when the DB
            // family column is unset.
            query
                .family
                .as_deref()
                .is_none_or(|family| effective_family(state, repo).as_deref() == Some(family))
        })
        .filter(|repo| {
            needle.as_deref().is_none_or(|needle| {
                repo.name.to_lowercase().contains(needle)
                    || repo
                        .description
                        .as_deref()
                        .is_some_and(|description| description.to_lowercase().contains(needle))
            })
        })
        .map(|repo| repo_summary(state, &repo))
        .collect();

    match query.sort.as_deref() {
        Some("name") => repositories.sort_by(|a, b| a.id.name.cmp(&b.id.name)),
        Some("open_prs") => {
            repositories.sort_by_key(|repo| std::cmp::Reverse(repo.open_pull_requests))
        }
        Some("failing_checks") => {
            repositories.sort_by_key(|repo| std::cmp::Reverse(repo.failing_checks));
        }
        // Default and "recent_activity": newest first (RFC3339 sorts
        // lexicographically).
        _ => repositories.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
    }

    RepositoryListResponse {
        generated_at: state.tui.generated_at.to_rfc3339(),
        total: repositories.len() as u64,
        repositories,
        facets: RepositoryFacets {
            hosts: vec!["jeryu".to_string()],
            owners: owners.into_iter().collect(),
            families: families.into_iter().collect(),
            languages: Vec::new(),
        },
    }
}

#[cfg(test)]
pub(super) fn repo_summaries(state: &WebState) -> Vec<RepositorySummary> {
    repo_summaries_for_user(state, None)
}

pub(super) fn repo_summaries_for_user(
    state: &WebState,
    account: Option<&AccountSummary>,
) -> Vec<RepositorySummary> {
    state
        .github
        .core()
        .list_repositories(None)
        .into_iter()
        .filter(|repo| {
            account.is_none_or(|account| {
                account.role == UserRole::Admin
                    || state.github.core().user_can_read_repo(
                        &account.login,
                        &repo.owner,
                        &repo.name,
                    )
            })
        })
        .map(|repo| repo_summary(state, &repo))
        .collect()
}

pub(super) fn repo_summary(state: &WebState, repo: &Repository) -> RepositorySummary {
    let split = state.split_catalog.classify(&repo.owner, &repo.name);
    let pulls = state
        .github
        .core()
        .list_pull_requests(&repo.owner, &repo.name, None)
        .unwrap_or_default();
    let checks = match state
        .github
        .core()
        .list_check_runs(&repo.owner, &repo.name, None)
    {
        Ok(runs) => runs.check_runs,
        Err(_) => Vec::new(),
    };
    let latest_score =
        state
            .github
            .core()
            .latest_jankurai_score(&repo.owner, &repo.name, &repo.default_branch);
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
        // Split-catalog classification wins; the persisted (DB) family is the
        // fallback for repos outside the manifest. Keep in sync with
        // `effective_family`.
        family: split
            .as_ref()
            .map(|(family, _)| family.clone())
            .or_else(|| repo.family.clone()),
        repo_role: split.map(|(_, role)| role),
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
        jankurai_score: latest_score.as_ref().and_then(|score| score.score),
        jankurai_decision: latest_score.as_ref().map(|score| score.decision.clone()),
        jankurai_scored_at: latest_score
            .as_ref()
            .map(|score| score.created_at.to_rfc3339()),
        mirror: mirror_status(&checks),
        // The smart-HTTP transport is mounted at /git/:owner/:repo (web.rs
        // git routes); /repos/* is the SPA + GitHub-compat surface and 404s
        // for git clients. Advertise the path git can actually clone.
        clone_http_url: Some(format!("/git/{}.git", repo.full_name)),
        clone_ssh_url: None,
        available_actions: vec![
            AvailableAction {
                action_id: "repo.open".to_string(),
                label: "Open".to_string(),
                risk: None,
            },
            AvailableAction {
                action_id: "repo.delete_registry".to_string(),
                label: "Remove from jeryu".to_string(),
                risk: Some("destructive".to_string()),
            },
            AvailableAction {
                action_id: "repo.delete_storage".to_string(),
                label: "Delete managed storage".to_string(),
                risk: Some("destructive".to_string()),
            },
        ],
    }
}

/// Grouping family one repository belongs to: the split-catalog
/// classification wins, falling back to the persisted repository family when
/// the catalog has no entry. This is the same precedence `repo_summary` uses
/// for its `family` field, so the `?family=` filter and the families facet
/// stay consistent with what the summaries expose.
fn effective_family(state: &WebState, repo: &Repository) -> Option<String> {
    state
        .split_catalog
        .classify(&repo.owner, &repo.name)
        .map(|(family, _)| family)
        .or_else(|| repo.family.clone())
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

/// Offsite mirror posture derived from `jeryu/github-mirror` bookkeeping
/// runs over the FULL check-run history (not sha-scoped — the newest mirror
/// attempt is meaningful regardless of which commit it pushed).
fn mirror_status(checks: &[CheckRun]) -> Option<RepositoryMirrorStatus> {
    let newer = |a: &CheckRun, b: &CheckRun| {
        (a.started_at, a.completed_at) >= (b.started_at, b.completed_at)
    };
    let mut latest: Option<&CheckRun> = None;
    let mut latest_success: Option<&CheckRun> = None;
    for check in checks
        .iter()
        .filter(|check| check.name == GITHUB_MIRROR_CHECK)
    {
        if latest.is_none_or(|held| newer(check, held)) {
            latest = Some(check);
        }
        if check.conclusion == Some(CheckConclusion::Success)
            && latest_success.is_none_or(|held| newer(check, held))
        {
            latest_success = Some(check);
        }
    }
    let latest = latest?;
    let run_time = |run: &CheckRun| run.completed_at.unwrap_or(run.started_at).to_rfc3339();
    Some(RepositoryMirrorStatus {
        configured: true,
        last_attempt_at: Some(run_time(latest)),
        last_attempt_ok: latest.conclusion == Some(CheckConclusion::Success),
        last_attempt_conclusion: latest.conclusion.as_ref().and_then(|conclusion| {
            serde_json::to_value(conclusion)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
        }),
        last_success_at: latest_success.map(run_time),
    })
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
    match state
        .github
        .core()
        .get_repository_readme(&repo.owner, &repo.name)
    {
        Ok(Some(markdown)) => markdown,
        Ok(None) | Err(_) => String::new(),
    }
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

pub(super) struct ApiErrorHint<'a> {
    pub(super) purpose: &'a str,
    pub(super) reason: &'a str,
    pub(super) common_fixes: &'a [&'a str],
    pub(super) docs_url: &'a str,
    pub(super) repair_hint: &'a str,
}

pub(super) fn api_error_with_hint(
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
