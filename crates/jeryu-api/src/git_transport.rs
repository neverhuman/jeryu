//! Git smart-HTTP transport handlers for the unified `jeryu serve`.
//!
//! These axum handlers mount `git clone`/`push` over HTTP by delegating to the
//! pure `jeryu_gitd::smart_http::SmartHttpServer`, run on the blocking pool so
//! the `git` subprocess never stalls an async worker. Kept in their own module
//! to keep `web.rs` focused on the REST/WS edge.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Path as AxumPath, Query as AxumQuery, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_gitd::RepoManager;
use jeryu_gitd::smart_http::{
    HttpRequest as GitHttpRequest, HttpResponse as GitHttpResponse, SmartHttpServer,
};

use crate::web::WebState;

async fn route_git(
    state: &WebState,
    peer: SocketAddr,
    method: &str,
    path: String,
    query: HashMap<String, String>,
    headers: &HeaderMap,
    body: Vec<u8>,
) -> AxumResponse {
    let mut forwarded = HashMap::new();
    if let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        forwarded.insert("authorization".to_string(), auth.to_string());
    }
    let request = GitHttpRequest {
        method: method.to_string(),
        path,
        query,
        headers: forwarded,
        body,
        is_loopback: peer.ip().is_loopback(),
    };
    // The gitd router shells out to `git` (blocking) and does file IO; run it on
    // the blocking pool so it never stalls an async worker.
    let manager = (*state.repo_manager).clone();
    let response =
        tokio::task::spawn_blocking(move || SmartHttpServer::new(manager).route(request))
            .await
            .expect("git smart-http task panicked");
    gitd_to_axum_response(&response)
}

/// Convert a gitd [`GitHttpResponse`] into an axum response, preserving status,
/// content type, and any extra headers (e.g. `WWW-Authenticate`).
fn gitd_to_axum_response(response: &GitHttpResponse) -> AxumResponse {
    let status =
        StatusCode::from_u16(response.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut out = (status, response.body().to_vec()).into_response();
    let map = out.headers_mut();
    if let Ok(content_type) = HeaderValue::from_str(response.content_type()) {
        map.insert(header::CONTENT_TYPE, content_type);
    }
    for (name, value) in response.extra_headers() {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            map.insert(name, value);
        }
    }
    out
}

pub(crate) async fn git_info_refs(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    AxumQuery(query): AxumQuery<HashMap<String, String>>,
    headers: HeaderMap,
) -> AxumResponse {
    route_git(
        &state,
        peer,
        "GET",
        format!("/{owner}/{repo}/info/refs"),
        query,
        &headers,
        Vec::new(),
    )
    .await
}

pub(crate) async fn git_upload_pack(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    route_git(
        &state,
        peer,
        "POST",
        format!("/{owner}/{repo}/git-upload-pack"),
        HashMap::new(),
        &headers,
        body.to_vec(),
    )
    .await
}

pub(crate) async fn git_receive_pack(
    State(state): State<Arc<WebState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    let manager = (*state.repo_manager).clone();
    let before = snapshot_refs(&manager, &owner, &repo);
    let origin_base_url = origin_base_url(&headers);
    let response = route_git(
        &state,
        peer,
        "POST",
        format!("/{owner}/{repo}/git-receive-pack"),
        HashMap::new(),
        &headers,
        body.to_vec(),
    )
    .await;
    // After a successful push, fire the push->CI bridge for any moved branch.
    if response.status().is_success() {
        let core = state.core.clone();
        let owner = owner.clone();
        let repo = repo.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let after = snapshot_refs(&manager, &owner, &repo);
            let updates = crate::ci_bridge::ref_updates(&before, &after);
            crate::ci_bridge::on_push(&core, &manager, &owner, &repo, &updates, &origin_base_url);
        })
        .await;
    }
    response
}

fn origin_base_url(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| !host.trim().is_empty())
        .map(|host| format!("http://{host}"))
        .or_else(|| std::env::var("JERYU_BASE").ok())
        .unwrap_or_else(|| "http://127.0.0.1:8787".to_string())
}

/// Snapshot a repo's refs. A repo that cannot be resolved or listed yields an
/// empty snapshot, so the post-push diff simply finds no updates.
fn snapshot_refs(manager: &RepoManager, owner: &str, repo: &str) -> Vec<jeryu_gitd::refs::GitRef> {
    manager
        .resolve_parts(owner, repo)
        .and_then(|resolved| {
            jeryu_gitd::refs::RefService::new(manager.clone()).list_refs(&resolved)
        })
        .unwrap_or_default()
}
