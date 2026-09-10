//! Work Tracker BFF routes backed by `jeryu-jira`.

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::{Extension, Json};
use jeryu_core::{AccountSummary, CreateIssueRequest, ForgeError, Repository, UserRole};
use jeryu_jira::{
    CreateWorkCommentRequest, CreateWorkItemRequest, CreateWorkLinkRequest, UpdateWorkItemRequest,
    WorkError, WorkFilter, WorkIssueLink, WorkItem, WorkItemListResponse, WorkPrincipal,
    WorkPrincipalKind, WorkRepository,
};
use serde::Deserialize;

use super::auth::forbidden;
use super::repositories::find_repo;
use super::{WebState, api_error};

#[derive(Debug, Default, Deserialize)]
pub(super) struct WorkListQuery {
    repo_id: Option<String>,
    status: Option<jeryu_jira::WorkStatus>,
    kind: Option<jeryu_jira::WorkItemKind>,
    priority: Option<jeryu_jira::WorkPriority>,
    assignee: Option<String>,
    label: Option<String>,
    search: Option<String>,
    q: Option<String>,
}

impl WorkListQuery {
    fn into_filter(self) -> WorkFilter {
        WorkFilter {
            repo_id: self.repo_id,
            status: self.status,
            kind: self.kind,
            priority: self.priority,
            assignee: self.assignee,
            label: self.label,
            search: self.search.or(self.q),
        }
    }
}

pub(super) async fn list(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    Query(query): Query<WorkListQuery>,
) -> AxumResponse {
    list_items(&state, &account, query.into_filter())
}

pub(super) async fn create(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    Json(request): Json<CreateWorkItemRequest>,
) -> AxumResponse {
    create_work_item(&state, &account, request)
}

pub(super) async fn detail(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(key): AxumPath<String>,
) -> AxumResponse {
    match state.work.detail(&key) {
        Ok(detail) if can_access_item(&state, &account, &detail.item, false) => {
            Json(detail).into_response()
        }
        Ok(_) => forbidden("Work access denied"),
        Err(error) => work_error(error),
    }
}

pub(super) async fn patch(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(key): AxumPath<String>,
    Json(request): Json<UpdateWorkItemRequest>,
) -> AxumResponse {
    if let Err(response) = authorized_item(&state, &account, &key, true) {
        return *response;
    }
    match state.work.patch(&key, request) {
        Ok(item) if can_access_item(&state, &account, &item, false) => Json(item).into_response(),
        Ok(_) => forbidden("Work access denied"),
        Err(error) => work_error(error),
    }
}

pub(super) async fn comment(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(key): AxumPath<String>,
    Json(mut request): Json<CreateWorkCommentRequest>,
) -> AxumResponse {
    if let Err(response) = authorized_item(&state, &account, &key, true) {
        return *response;
    }
    request.author = Some(WorkPrincipal {
        kind: WorkPrincipalKind::Human,
        id: account.login,
        display_name: Some(account.display_name),
    });
    match state.work.add_comment(&key, request) {
        Ok(comment) => (StatusCode::CREATED, Json(comment)).into_response(),
        Err(error) => work_error(error),
    }
}

pub(super) async fn link(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(key): AxumPath<String>,
    Json(mut request): Json<CreateWorkLinkRequest>,
) -> AxumResponse {
    let item = match authorized_item(&state, &account, &key, true) {
        Ok(item) => item,
        Err(response) => return *response,
    };
    // Link contracts carry names, not target UUIDs. Only the owning repository
    // has an immutable identity that ordinary-user authorization can validate.
    if account.role != UserRole::Admin
        && !links_match_repository(
            item.repo.as_ref(),
            request.issue.as_ref(),
            request.pull_request.as_ref().into_iter(),
        )
    {
        return forbidden("cross-repository Work links require an administrator");
    }
    if let Some(issue) = &mut request.issue {
        if !can_access_named_repo(&state, &account, &issue.owner, &issue.repo, false) {
            return forbidden("linked repository access denied");
        }
        if state
            .core
            .get_issue(&issue.owner, &issue.repo, issue.number)
            .is_err()
        {
            return api_error(StatusCode::NOT_FOUND, "not_found", "linked issue not found");
        }
        issue.url = Some(format!(
            "/repos/jeryu/{}/{}/issues#{}",
            issue.owner, issue.repo, issue.number
        ));
    }
    if let Some(pull) = &mut request.pull_request {
        if !can_access_named_repo(&state, &account, &pull.owner, &pull.repo, false) {
            return forbidden("linked repository access denied");
        }
        if state
            .core
            .get_pull_request(&pull.owner, &pull.repo, pull.number)
            .is_err()
        {
            return api_error(
                StatusCode::NOT_FOUND,
                "not_found",
                "linked pull request not found",
            );
        }
        pull.url = Some(format!(
            "/repos/jeryu/{}/{}/pulls/{}",
            pull.owner, pull.repo, pull.number
        ));
    }
    match state.work.link(&key, request) {
        Ok(item) if can_access_item(&state, &account, &item, false) => Json(item).into_response(),
        Ok(_) => forbidden("Work access denied"),
        Err(error) => work_error(error),
    }
}

pub(super) async fn repo_list(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<WorkListQuery>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    if !can_access_repo(&state, &account, &repo, false) {
        return forbidden("repository access denied");
    }
    let mut filter = query.into_filter();
    filter.repo_id = Some(repo.id.to_string());
    list_items(&state, &account, filter)
}

fn list_items(state: &WebState, account: &AccountSummary, filter: WorkFilter) -> AxumResponse {
    match state.work.list(filter) {
        Ok(mut items) => {
            items.retain(|item| can_access_item(state, account, item, false));
            Json(WorkItemListResponse {
                total: items.len(),
                items,
            })
            .into_response()
        }
        Err(error) => work_error(error),
    }
}

pub(super) async fn repo_create(
    State(state): State<std::sync::Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(id): AxumPath<String>,
    Json(mut request): Json<CreateWorkItemRequest>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    request.repo = Some(work_repo(&repo));
    create_work_item(&state, &account, request)
}

fn create_work_item(
    state: &WebState,
    account: &AccountSummary,
    mut request: CreateWorkItemRequest,
) -> AxumResponse {
    if let Some(repo_ref) = request.repo.clone() {
        let Some(repo) = find_repo(state, &repo_ref.id) else {
            return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
        };
        if !can_access_repo(state, account, &repo, true) {
            return forbidden("repository access denied");
        }
        if repo_ref != work_repo(&repo) {
            return api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_input",
                "use the canonical repository identity returned by the repository API",
            );
        }
        request.repo = Some(work_repo(&repo));
        if let Err(error) = request.validate() {
            return work_error(error);
        }
        let issue = match state.core.create_issue(
            &repo.owner,
            &repo.name,
            &account.login,
            CreateIssueRequest {
                title: request.title.clone(),
                body: request.body.clone(),
                labels: request.labels.clone(),
                assignees: request.assignees.iter().map(|p| p.id.clone()).collect(),
                milestone: None,
            },
        ) {
            Ok(issue) => issue,
            Err(ForgeError::Validation(reason)) => {
                return api_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_input", &reason);
            }
            Err(ForgeError::NotFound(_)) => {
                return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
            }
            Err(error) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_failed",
                    &format!("could not create linked issue: {error}"),
                );
            }
        };
        let link = WorkIssueLink {
            owner: issue.owner.clone(),
            repo: issue.repo.clone(),
            number: issue.number,
            url: Some(format!(
                "/repos/jeryu/{}/{}/issues#{}",
                issue.owner, issue.repo, issue.number
            )),
        };
        match state.work.create_with_issue(request, link) {
            Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
            Err(error) => work_error(error),
        }
    } else {
        // Work has no separate user/team namespace grants. Unbound records
        // therefore remain in the existing administrator namespace.
        if account.role != UserRole::Admin {
            return forbidden("unbound Work requires an administrator");
        }
        match state.work.create(request) {
            Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
            Err(error) => work_error(error),
        }
    }
}

fn can_access_repo(
    state: &WebState,
    account: &AccountSummary,
    repo: &Repository,
    write: bool,
) -> bool {
    account.role == UserRole::Admin
        || if write {
            state
                .core
                .user_can_write_repo(&account.login, &repo.owner, &repo.name)
        } else {
            state
                .core
                .user_can_read_repo(&account.login, &repo.owner, &repo.name)
        }
}

fn can_access_named_repo(
    state: &WebState,
    account: &AccountSummary,
    owner: &str,
    name: &str,
    write: bool,
) -> bool {
    find_repo(state, &format!("{owner}/{name}"))
        .is_some_and(|repo| can_access_repo(state, account, &repo, write))
}

fn can_access_item(
    state: &WebState,
    account: &AccountSummary,
    item: &WorkItem,
    write: bool,
) -> bool {
    if account.role == UserRole::Admin {
        return true;
    }
    let Some(reference) = &item.repo else {
        return false;
    };
    if reference.host != "jeryu" {
        return false;
    }
    let Some(repo) = find_repo(state, &reference.id) else {
        return false;
    };
    work_repo(&repo) == *reference
        && can_access_repo(state, account, &repo, write)
        && links_match_repository(
            Some(reference),
            item.issue.as_ref(),
            item.pull_requests.iter(),
        )
}

fn links_match_repository<'a>(
    repo: Option<&WorkRepository>,
    issue: Option<&WorkIssueLink>,
    mut pulls: impl Iterator<Item = &'a jeryu_jira::WorkPullRequestLink>,
) -> bool {
    let Some(repo) = repo else {
        return false;
    };
    issue.is_none_or(|issue| issue.owner == repo.owner && issue.repo == repo.name)
        && pulls.all(|pull| pull.owner == repo.owner && pull.repo == repo.name)
}

fn authorized_item(
    state: &WebState,
    account: &AccountSummary,
    key: &str,
    write: bool,
) -> Result<WorkItem, Box<AxumResponse>> {
    let item = state
        .work
        .get(key)
        .map_err(|error| Box::new(work_error(error)))?;
    if !can_access_item(state, account, &item, write) {
        return Err(Box::new(forbidden("Work access denied")));
    }
    Ok(item)
}

fn work_repo(repo: &Repository) -> WorkRepository {
    WorkRepository {
        id: repo.id.to_string(),
        host: "jeryu".to_string(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
    }
}

fn work_error(error: WorkError) -> AxumResponse {
    match error {
        WorkError::Validation(reason) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_input", &reason)
        }
        WorkError::NotFound(_) => api_error(StatusCode::NOT_FOUND, "not_found", "work not found"),
        WorkError::Conflict(reason) => api_error(StatusCode::CONFLICT, "conflict", &reason),
        WorkError::Storage(reason) | WorkError::Serialization(reason) => {
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "storage_failed", &reason)
        }
    }
}
