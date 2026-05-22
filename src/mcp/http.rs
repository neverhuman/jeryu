//! Owner: MCP adapter for external coding agents
//! Proof: `cargo check -p jeryu --message-format=json` and `cargo test -p jeryu --lib mcp`
//! Invariants: HTTP transport is loopback-only, validates MCP session headers, and routes
//!             tool execution through the same capability policy path as stdio.

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use anyhow::{Result, bail};
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use tokio::sync::Mutex;

use super::{
    MCP_PROTOCOL_VERSION,
    core::{JsonRpcRequest, McpCore, McpSessionState},
};

#[path = "http_support.rs"]
mod http_support;
use http_support::{
    header_value, http_error, http_jsonrpc_error, http_jsonrpc_response, validate_mcp_http_headers,
};

#[path = "http_post.rs"]
mod http_post;
use http_post::handle_mcp_post;

#[cfg(test)]
pub(crate) fn is_loopback_origin(origin: &str) -> bool {
    http_support::is_loopback_origin(origin)
}

#[derive(Clone)]
pub(crate) struct McpHttpState {
    core: McpCore,
    sessions: Arc<Mutex<HashMap<String, McpSessionState>>>,
}

impl McpHttpState {
    pub(crate) fn new(client: crate::gitlab_client::GitlabClient) -> Self {
        Self {
            core: McpCore::new(client),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub(crate) fn mcp_router(state: Arc<McpHttpState>) -> Router {
    Router::new()
        .route(
            "/mcp",
            post(handle_mcp_post)
                .delete(handle_mcp_delete)
                .get(handle_mcp_get),
        )
        .with_state(state)
}

pub async fn start_mcp_http(client: crate::gitlab_client::GitlabClient, bind: &str) -> Result<()> {
    let addr: SocketAddr = bind
        .parse()
        .map_err(|err| anyhow::anyhow!("invalid MCP HTTP bind '{}': {err}", bind))?;
    if !addr.ip().is_loopback() {
        bail!("MCP HTTP bind must be loopback-only; got {addr}");
    }

    let state = Arc::new(McpHttpState::new(client));
    let app = mcp_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "MCP HTTP server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

pub(crate) async fn handle_mcp_get() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [("Allow", "POST, DELETE")],
        "Streamable HTTP GET is not enabled for jeryu MCP",
    )
        .into_response()
}

pub(crate) async fn handle_mcp_delete(
    State(state): State<Arc<McpHttpState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = validate_mcp_http_headers(&headers, false) {
        return response;
    }

    let Some(session_id) = header_value(&headers, "Mcp-Session-Id") else {
        return http_error(StatusCode::BAD_REQUEST, "Mcp-Session-Id header is required");
    };

    let mut sessions = state.sessions.lock().await;
    if sessions.remove(session_id).is_some() {
        (
            StatusCode::NO_CONTENT,
            [("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)],
            (),
        )
            .into_response()
    } else {
        http_error(StatusCode::NOT_FOUND, "unknown MCP session")
    }
}
