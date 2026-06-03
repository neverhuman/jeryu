//! General local-web helpers that are not repository-specific.

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method as HttpMethod, StatusCode, header};
use axum::response::{Html, IntoResponse, Response as AxumResponse};
use jeryu_readmodel::contracts::{RenderedMarkdown, Viewer, WebBootstrap, WebFeatureFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use tokio::fs;

use super::markdown::render_markdown;
use super::permissions::permissions;
use super::repositories::repo_summaries;
use crate::{Method, Response as GithubResponse};

#[derive(Debug, Deserialize)]
pub(super) struct MarkdownRequest {
    #[serde(default)]
    markdown: String,
}

pub(super) async fn markdown_render(
    Json(request): Json<MarkdownRequest>,
) -> Json<RenderedMarkdown> {
    Json(render_markdown(&request.markdown))
}

pub(super) async fn graphql(
    State(state): State<std::sync::Arc<super::WebState>>,
    body: Bytes,
) -> AxumResponse {
    let body = std::str::from_utf8(&body).unwrap_or_default();
    github_response(state.github.handle(Method::Post, "/graphql", body))
}

/// Accept-aware `/repos` entrypoint that serves the SPA shell to browser
/// navigations and the GitHub-compatible REST edge to API clients.
pub(super) async fn repo_entry(
    State(state): State<std::sync::Arc<super::WebState>>,
    method: HttpMethod,
    headers: HeaderMap,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
    body: Bytes,
) -> AxumResponse {
    if method == HttpMethod::GET {
        let path = uri.path();
        if is_browser_navigation(&headers)
            || is_browser_repo_route(path)
            || (is_repo_index(path) && !accepts_json(&headers))
        {
            return spa_shell_response(&state.spa_dir).await;
        }
    }
    github_forward_request(state, method, uri, body).await
}

/// Forwards a GitHub-compatible REST request to the in-process [`GithubRouter`],
/// which routes by `(method, path)` and renders GitHub-shaped JSON. The original
/// request path is forwarded verbatim so the dispatcher's segment matching works
/// unchanged; an unsupported HTTP verb returns a GitHub-shaped `405`.
pub(super) async fn github_forward(
    State(state): State<std::sync::Arc<super::WebState>>,
    method: HttpMethod,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
    body: Bytes,
) -> AxumResponse {
    github_forward_request(state, method, uri, body).await
}

async fn github_forward_request(
    state: std::sync::Arc<super::WebState>,
    method: HttpMethod,
    uri: axum::http::Uri,
    body: Bytes,
) -> AxumResponse {
    let Some(method) = map_method(&method) else {
        return guided_github_edge_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "Method Not Allowed",
            "route unsupported GitHub-compatible REST method",
            "the Jeryu GitHub edge accepts GET, POST, and PUT for the guided compatibility subset",
            uri.path(),
        );
    };
    let path_and_query = uri
        .path_and_query()
        .map_or_else(|| uri.path().to_string(), ToString::to_string);
    let body = std::str::from_utf8(&body).unwrap_or_default();
    github_response(state.github.handle(method, &path_and_query, body))
}

fn accepts_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("application/json"))
}

fn is_repo_index(path: &str) -> bool {
    matches!(path, "/repos" | "/repos/")
}

fn is_browser_navigation(headers: &HeaderMap) -> bool {
    let mode = headers
        .get("sec-fetch-mode")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if mode == "navigate" {
        return true;
    }
    headers
        .get("sec-fetch-dest")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|dest| dest.eq_ignore_ascii_case("document"))
}

fn is_browser_repo_route(path: &str) -> bool {
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    match (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) {
        (Some("repos"), Some(_host), Some(_repo), None, None) => true,
        (Some("repos"), Some(_host), Some(_repo), Some("code"), None) => true,
        (Some("repos"), Some(_host), Some(_repo), Some("settings"), Some("merge")) => true,
        (Some("repos"), Some(_host), Some(_repo), Some("blob"), Some(_)) => true,
        (Some("repos"), Some(_host), Some(_repo), Some("tree"), Some(_)) => true,
        (Some("repos"), Some(_host), Some(_repo), Some("pulls"), Some(number)) => {
            number.chars().all(|ch| ch.is_ascii_digit())
        }
        _ => false,
    }
}

async fn spa_shell_response(spa_dir: &Path) -> AxumResponse {
    match fs::read_to_string(spa_dir.join("index.html")).await {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("failed to load SPA shell from {}: {err}", spa_dir.display()),
        )
            .into_response(),
    }
}

pub(super) fn bootstrap_payload(
    state: &super::WebState,
) -> Result<WebBootstrap, serde_json::Error> {
    let repos = repo_summaries(state);
    let tui = serialize_payload(&super::workcells::live_tui(state))?;
    Ok(WebBootstrap {
        generated_at: state.tui.generated_at.to_rfc3339(),
        schema_version: "0.1.0-alpha".to_string(),
        viewer: Viewer {
            id: "local-operator".to_string(),
            login: "local".to_string(),
            display_name: Some("Local Operator".to_string()),
            avatar_url: None,
            global_permissions: permissions(),
        },
        tui,
        recent_repositories: repos.into_iter().take(10).collect(),
        websocket_url: "/api/v1/ws".to_string(),
        feature_flags: WebFeatureFlags {
            repo_create: false,
            settings_write: false,
            merge_write: false,
            markdown_html: true,
            agents: false,
            mcp: true,
            workcells: true,
        },
    })
}

pub(super) fn serialize_payload<T: Serialize>(value: &T) -> Result<Value, serde_json::Error> {
    serde_json::to_value(value)
}

/// Maps the HTTP verbs the GitHub edge supports to the dispatcher's [`Method`].
pub(super) fn map_method(method: &HttpMethod) -> Option<Method> {
    match *method {
        HttpMethod::GET => Some(Method::Get),
        HttpMethod::POST => Some(Method::Post),
        HttpMethod::PUT => Some(Method::Put),
        _ => None,
    }
}

fn guided_github_edge_response(
    status: StatusCode,
    message: &str,
    purpose: &str,
    reason: &str,
    path: &str,
) -> AxumResponse {
    (
        status,
        Json(json!({
            "message": message,
            "documentation_url": "/docs/rest",
            "jeryu_repair_hint": {
                "purpose": purpose,
                "reason": reason,
                "common_fixes": [
                    "retry with one of the listed GitHub-compatible REST routes",
                    "use /.jeryu/capabilities to choose a typed jeryu.* MCP tool",
                    "add a conformance test before widening the compatibility subset"
                ],
                "docs_url": "/docs/rest",
                "repair_hint": "prefer the listed Jeryu MCP/API alternatives, then rerun cargo test -p jeryu-api --features web"
            },
            "jeryu_mcp_tools": super::MCP_GUIDANCE_TOOLS,
            "jeryu_api_routes": [
                "GET /user",
                "GET /repos",
                "GET /repos/{owner}/{repo}",
                "GET /repos/{owner}/{repo}/pulls",
                "GET /repos/{owner}/{repo}/issues",
                "GET /repos/{owner}/{repo}/commits/{ref}/status",
                "GET /repos/{owner}/{repo}/commits/{ref}/check-runs",
                "POST /graphql"
            ],
            "path": path,
        })),
    )
        .into_response()
}

fn github_response(response: GithubResponse) -> AxumResponse {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut axum_response = (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        response.body,
    )
        .into_response();
    let headers = axum_response.headers_mut();
    for (name, value) in response.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            headers.insert(name, value);
        }
    }
    axum_response
}
