//! Axum HTTP transport (port of `http.rs`): loopback-only bind, session map, router.

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

use crate::MCP_PROTOCOL_VERSION;
use crate::backend::ToolBackend;
use crate::core::{McpCore, McpSessionState};

mod post;
mod support;

use post::handle_mcp_post;
use support::{header_value, http_error, validate_mcp_http_headers};

/// Strict loopback-origin classifier (exposed for tests and operator checks).
pub fn is_loopback_origin(origin: &str) -> bool {
    support::is_loopback_origin(origin)
}

/// Shared HTTP server state: the core (cloneable, backend-holding) + the session map.
#[derive(Clone)]
pub struct McpHttpState {
    core: McpCore,
    sessions: Arc<Mutex<HashMap<String, McpSessionState>>>,
}

impl McpHttpState {
    pub fn new(backend: Arc<dyn ToolBackend>) -> Self {
        Self {
            core: McpCore::new(backend),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub fn mcp_router(state: Arc<McpHttpState>) -> Router {
    Router::new()
        .route(
            "/mcp",
            post(handle_mcp_post)
                .delete(handle_mcp_delete)
                .get(handle_mcp_get),
        )
        .with_state(state)
}

/// Start the loopback-only HTTP MCP server. Rejects any non-loopback bind address.
pub async fn start_mcp_http(backend: Arc<dyn ToolBackend>, bind: &str) -> Result<()> {
    let addr: SocketAddr = bind
        .parse()
        .map_err(|err| anyhow::anyhow!("invalid MCP HTTP bind '{bind}': {err}"))?;
    if !addr.ip().is_loopback() {
        bail!("MCP HTTP bind must be loopback-only; got {addr}");
    }

    let state = Arc::new(McpHttpState::new(backend));
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
