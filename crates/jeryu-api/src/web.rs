//! Axum HTTP/WebSocket edge for the local live Jeryu API.

mod markdown;
mod permissions;

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response as AxumResponse};
use axum::routing::{get, post};
use axum::{Json, Router as AxumRouter};
use futures_util::StreamExt;
use jeryu_core::{CheckConclusion, ForgeCore, PullRequestState, Repository};
use jeryu_readmodel::contracts::{
    AvailableAction, BlobEncoding, BlobResponse, EntityHandle, RefKind, RefSelectorItem,
    RenderedMarkdown, RepositoryFacets, RepositoryId, RepositoryListResponse, RepositorySummary,
    RepositoryVisibility, ServerWsMessage, TreeEntry, Viewer, WebBootstrap, WebFeatureFlags,
};
use jeryu_readmodel::{TuiReadModel, sample_read_model};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};

use crate::{GithubRouter, Method, Response as GithubResponse};
use markdown::render_markdown;
use permissions::permissions;

const WS_PROTOCOL: &str = "jeryu.ws.v1";

#[derive(Clone, Debug)]
pub struct WebServerConfig {
    pub bind: SocketAddr,
    pub spa_dir: PathBuf,
    pub data_dir: PathBuf,
}

#[derive(Clone)]
struct WebState {
    github: GithubRouter,
    tui: TuiReadModel,
}

impl WebState {
    fn new(core: ForgeCore) -> Self {
        Self {
            github: GithubRouter::with_core(core),
            tui: sample_read_model(),
        }
    }
}

pub async fn serve(config: WebServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&config.data_dir)?;
    let db_path = config.data_dir.join("forge.sqlite");
    let core = ForgeCore::open_sqlite(db_path)?;
    let app = app(WebState::new(core), &config.spa_dir);
    let listener = TcpListener::bind(config.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn app(state: WebState, spa_dir: &Path) -> AxumRouter {
    let spa = ServeDir::new(spa_dir).fallback(ServeFile::new(spa_dir.join("index.html")));
    AxumRouter::new()
        .route("/health", get(health))
        .route("/api/v1/bootstrap", get(bootstrap))
        .route("/api/v1/bootstrap.tui", get(bootstrap_tui))
        .route("/api/v1/repos", get(repos))
        .route("/api/v1/repos/:id", get(repo_detail))
        .route("/api/v1/repos/:id/refs", get(repo_refs))
        .route("/api/v1/repos/:id/tree", get(repo_tree))
        .route("/api/v1/repos/:id/blob", get(repo_blob))
        .route("/api/v1/repos/:id/raw", get(repo_raw))
        .route("/api/v1/repos/:id/readme", get(repo_readme))
        .route("/api/v1/markdown/render", post(markdown_render))
        .route("/api/v1/ws", get(ws))
        .route("/graphql", post(graphql))
        .fallback_service(spa)
        .with_state(Arc::new(state))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "jeryu-api" }))
}

async fn bootstrap(State(state): State<Arc<WebState>>) -> Json<WebBootstrap> {
    Json(bootstrap_payload(&state))
}

async fn bootstrap_tui(State(state): State<Arc<WebState>>) -> Json<TuiReadModel> {
    Json(state.tui.clone())
}

async fn repos(State(state): State<Arc<WebState>>) -> Json<RepositoryListResponse> {
    Json(repo_list_response(&state))
}

async fn repo_detail(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    match find_repo(&state, &id) {
        Some(repo) => Json(repo_summary(&state, &repo)).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "not_found", "repository not found"),
    }
}

async fn repo_refs(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    Json(vec![RefSelectorItem {
        name: repo.default_branch,
        sha: "unknown".to_string(),
        kind: RefKind::Branch,
        protected: state
            .github
            .core()
            .get_branch_protection(&repo.owner, &repo.name, "main")
            .is_ok(),
    }])
    .into_response()
}

async fn repo_tree(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    if find_repo(&state, &id).is_none() {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    }
    Json(Vec::<TreeEntry>::new()).into_response()
}

async fn repo_blob(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    Json(BlobResponse {
        repo: repo_id(&repo),
        path: "README.md".to_string(),
        ref_name: repo.default_branch,
        sha: "unknown".to_string(),
        size_bytes: 0,
        mime: "text/markdown".to_string(),
        encoding: BlobEncoding::Utf8,
        text: Some(String::new()),
        base64: None,
        rendered_markdown: Some(render_markdown(&format!("# {}\n", repo.full_name))),
        is_binary: false,
    })
    .into_response()
}

async fn repo_raw(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    Html(format!("# {}\n", repo.full_name)).into_response()
}

async fn repo_readme(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    Json(render_markdown(&format!(
        "# {}\n\nRepository metadata is live. Source import has not attached a README yet.\n",
        repo.full_name
    )))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct MarkdownRequest {
    #[serde(default)]
    markdown: String,
}

async fn markdown_render(Json(request): Json<MarkdownRequest>) -> Json<RenderedMarkdown> {
    Json(render_markdown(&request.markdown))
}

async fn graphql(State(state): State<Arc<WebState>>, body: Bytes) -> AxumResponse {
    let body = std::str::from_utf8(&body).unwrap_or_default();
    github_response(state.github.handle(Method::Post, "/graphql", body))
}

async fn ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut socket: WebSocket) {
    let _ = send_server_message(&mut socket, hello_message()).await;
    while let Some(message) = socket.next().await {
        match message {
            Ok(Message::Text(text)) => {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    match value.get("type").and_then(Value::as_str) {
                        Some("ping") => {
                            let nonce = value
                                .get("nonce")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let _ = send_server_message(
                                &mut socket,
                                ServerWsMessage::Pong {
                                    nonce,
                                    server_time: server_time(),
                                },
                            )
                            .await;
                        }
                        Some("hello") => {
                            let _ = send_server_message(&mut socket, hello_message()).await;
                        }
                        Some("ack" | "subscribe" | "unsubscribe") => {}
                        _ => {
                            let _ = send_server_message(
                                &mut socket,
                                ServerWsMessage::Error {
                                    code: "unknown_message".to_string(),
                                    message: "unsupported websocket message type".to_string(),
                                },
                            )
                            .await;
                        }
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = socket.send(Message::Pong(payload)).await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
}

async fn send_server_message(socket: &mut WebSocket, message: ServerWsMessage) -> Result<(), ()> {
    let encoded = serde_json::to_string(&message).map_err(|_| ())?;
    socket.send(Message::Text(encoded)).await.map_err(|_| ())
}

fn hello_message() -> ServerWsMessage {
    ServerWsMessage::Hello {
        server_time: server_time(),
        current_seq: 0,
        protocol: WS_PROTOCOL.to_string(),
    }
}

fn bootstrap_payload(state: &WebState) -> WebBootstrap {
    let repos = repo_summaries(state);
    WebBootstrap {
        generated_at: state.tui.generated_at.to_rfc3339(),
        schema_version: "0.1.0-alpha".to_string(),
        viewer: Viewer {
            id: "local-operator".to_string(),
            login: "local".to_string(),
            display_name: Some("Local Operator".to_string()),
            avatar_url: None,
            global_permissions: permissions(),
        },
        tui: serde_json::to_value(&state.tui).unwrap_or_else(|_| json!({})),
        recent_repositories: repos.into_iter().take(10).collect(),
        websocket_url: "/api/v1/ws".to_string(),
        feature_flags: WebFeatureFlags {
            repo_create: false,
            settings_write: false,
            merge_write: false,
            markdown_html: true,
            agents: false,
            mcp: false,
        },
    }
}

fn repo_list_response(state: &WebState) -> RepositoryListResponse {
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

fn repo_summaries(state: &WebState) -> Vec<RepositorySummary> {
    state
        .github
        .core()
        .list_repositories(None)
        .into_iter()
        .map(|repo| repo_summary(state, &repo))
        .collect()
}

fn repo_summary(state: &WebState, repo: &Repository) -> RepositorySummary {
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
        health: if checks
            .iter()
            .any(|check| check.conclusion == Some(CheckConclusion::Failure))
        {
            "warning".to_string()
        } else {
            "healthy".to_string()
        },
        open_pull_requests: pulls
            .iter()
            .filter(|pr| {
                !matches!(
                    pr.state,
                    PullRequestState::Closed | PullRequestState::Merged
                )
            })
            .count() as u32,
        failing_checks: checks
            .iter()
            .filter(|check| check.conclusion == Some(CheckConclusion::Failure))
            .count() as u32,
        running_jobs: checks
            .iter()
            .filter(|check| check.status == jeryu_core::CheckRunStatus::InProgress)
            .count() as u32,
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

fn repo_id(repo: &Repository) -> RepositoryId {
    RepositoryId {
        id: repo.id.to_string(),
        host: "jeryu".to_string(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
    }
}

fn find_repo(state: &WebState, id: &str) -> Option<Repository> {
    state
        .github
        .core()
        .list_repositories(None)
        .into_iter()
        .find(|repo| repo.id.to_string() == id || repo.full_name == id)
}

pub(super) fn server_time() -> String {
    chrono_like_now()
}

fn chrono_like_now() -> String {
    jeryu_readmodel::TuiReadModel::default()
        .generated_at
        .to_rfc3339()
}

fn api_error(status: StatusCode, code: &str, message: &str) -> AxumResponse {
    (status, Json(json!({ "code": code, "message": message }))).into_response()
}

fn github_response(response: GithubResponse) -> AxumResponse {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        response.body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_core::CreateRepositoryRequest;

    #[test]
    fn bootstrap_and_repo_list_reflect_core_repositories() {
        let core = ForgeCore::new();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: true,
                description: Some("forge".to_string()),
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        let state = WebState::new(core);
        let bootstrap = bootstrap_payload(&state);
        assert_eq!(bootstrap.websocket_url, "/api/v1/ws");
        assert_eq!(bootstrap.recent_repositories.len(), 1);
        let repos = repo_list_response(&state);
        assert_eq!(repos.total, 1);
        assert_eq!(repos.repositories[0].id.owner, "alice");
    }

    #[test]
    fn markdown_renderer_escapes_html_and_builds_toc() {
        let rendered = render_markdown("# Hello <world>\n\nbody");
        assert!(rendered.html.contains("&lt;world&gt;"));
        assert_eq!(rendered.toc[0].id, "hello-world");
    }
}
