//! Work Tracker BFF routes backed by `jeryu-jira`.

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::{CreateIssueRequest, ForgeError, Repository};
use jeryu_jira::{
    CreateWorkCommentRequest, CreateWorkItemRequest, CreateWorkLinkRequest, UpdateWorkItemRequest,
    WorkError, WorkFilter, WorkIssueLink, WorkItemListResponse, WorkRepository,
};
use serde::Deserialize;

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
    Query(query): Query<WorkListQuery>,
) -> AxumResponse {
    match state.work.list(query.into_filter()) {
        Ok(items) => Json(WorkItemListResponse {
            total: items.len(),
            items,
        })
        .into_response(),
        Err(error) => work_error(error),
    }
}

pub(super) async fn create(
    State(state): State<std::sync::Arc<WebState>>,
    Json(request): Json<CreateWorkItemRequest>,
) -> AxumResponse {
    create_work_item(&state, request)
}

pub(super) async fn detail(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(key): AxumPath<String>,
) -> AxumResponse {
    match state.work.detail(&key) {
        Ok(detail) => Json(detail).into_response(),
        Err(error) => work_error(error),
    }
}

pub(super) async fn patch(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(key): AxumPath<String>,
    Json(request): Json<UpdateWorkItemRequest>,
) -> AxumResponse {
    match state.work.patch(&key, request) {
        Ok(item) => Json(item).into_response(),
        Err(error) => work_error(error),
    }
}

pub(super) async fn comment(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(key): AxumPath<String>,
    Json(request): Json<CreateWorkCommentRequest>,
) -> AxumResponse {
    match state.work.add_comment(&key, request) {
        Ok(comment) => (StatusCode::CREATED, Json(comment)).into_response(),
        Err(error) => work_error(error),
    }
}

pub(super) async fn link(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(key): AxumPath<String>,
    Json(request): Json<CreateWorkLinkRequest>,
) -> AxumResponse {
    match state.work.link(&key, request) {
        Ok(item) => Json(item).into_response(),
        Err(error) => work_error(error),
    }
}

pub(super) async fn repo_list(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<WorkListQuery>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    let mut filter = query.into_filter();
    filter.repo_id = Some(repo.id.to_string());
    match state.work.list(filter) {
        Ok(items) => Json(WorkItemListResponse {
            total: items.len(),
            items,
        })
        .into_response(),
        Err(error) => work_error(error),
    }
}

pub(super) async fn repo_create(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    Json(mut request): Json<CreateWorkItemRequest>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    request.repo = Some(work_repo(&repo));
    create_work_item(&state, request)
}

fn create_work_item(state: &WebState, mut request: CreateWorkItemRequest) -> AxumResponse {
    if let Some(repo_ref) = request.repo.clone() {
        let Some(repo) = find_repo(state, &repo_ref.id)
            .or_else(|| find_repo(state, &format!("{}/{}", repo_ref.owner, repo_ref.name)))
        else {
            return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
        };
        request.repo = Some(work_repo(&repo));
        let issue = match state.core.create_issue(
            &repo.owner,
            &repo.name,
            "local",
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
        match state.work.create(request) {
            Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
            Err(error) => work_error(error),
        }
    }
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
