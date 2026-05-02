//! Owner: MCP adapter for external coding agents
//! Proof: `cargo check -p jeryu --message-format=json` and `cargo test -p jeryu --lib mcp`
//! Invariants: HTTP transport is loopback-only, validates MCP session headers, and routes
//!             tool execution through the same capability policy path as stdio.

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use anyhow::{Result, bail};
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde_json::Value;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{
    MCP_PROTOCOL_VERSION,
    core::{JsonRpcRequest, McpCore, McpSessionState, jsonrpc_error},
};

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

pub(crate) async fn handle_mcp_post(
    State(state): State<Arc<McpHttpState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(response) = validate_mcp_http_headers(&headers, true) {
        return response;
    }

    let raw = match serde_json::from_slice::<Value>(&body) {
        Ok(value) => value,
        Err(err) => return http_jsonrpc_error(None, -32700, &format!("parse error: {err}")),
    };
    if raw.is_array() {
        return http_error(
            StatusCode::BAD_REQUEST,
            "batch requests are not supported over MCP HTTP",
        );
    }

    let request: JsonRpcRequest = match serde_json::from_value(raw) {
        Ok(value) => value,
        Err(err) => return http_jsonrpc_error(None, -32600, &format!("invalid request: {err}")),
    };

    let method_header = match header_value(&headers, "Mcp-Method") {
        Some(value) => value.to_string(),
        None => return http_error(StatusCode::BAD_REQUEST, "Mcp-Method header is required"),
    };
    if method_header != request.method {
        return http_error(
            StatusCode::BAD_REQUEST,
            "Mcp-Method header does not match body",
        );
    }

    if request.method == "initialize" {
        if header_value(&headers, "Mcp-Session-Id").is_some() {
            return http_error(
                StatusCode::BAD_REQUEST,
                "Mcp-Session-Id must be omitted for initialize",
            );
        }
        let mut session = McpSessionState::new();
        let response = state.core.handle_request(&mut session, request).await;
        let Some(result) = response else {
            return StatusCode::NO_CONTENT.into_response();
        };

        let session_id = Uuid::new_v4().to_string();
        let mut sessions = state.sessions.lock().await;
        sessions.insert(session_id.clone(), session);

        return http_jsonrpc_response(
            StatusCode::OK,
            result,
            Some((
                header::HeaderName::from_static("mcp-session-id"),
                session_id,
            )),
        );
    }

    let Some(protocol_version) = header_value(&headers, "MCP-Protocol-Version") else {
        return http_error(
            StatusCode::BAD_REQUEST,
            "MCP-Protocol-Version header is required",
        );
    };
    if protocol_version != MCP_PROTOCOL_VERSION {
        return http_error(
            StatusCode::BAD_REQUEST,
            "unsupported MCP-Protocol-Version header",
        );
    }

    let Some(session_id) = header_value(&headers, "Mcp-Session-Id") else {
        return http_error(StatusCode::BAD_REQUEST, "Mcp-Session-Id header is required");
    };

    if request.method == "tools/call" {
        let Some(name_header) = header_value(&headers, "Mcp-Name") else {
            return http_error(StatusCode::BAD_REQUEST, "Mcp-Name header is required");
        };
        if let Some(params) = request.params.as_ref()
            && let Some(name) = params.get("name").and_then(Value::as_str)
                && name != name_header {
                    return http_error(
                        StatusCode::BAD_REQUEST,
                        "Mcp-Name header does not match body",
                    );
                }
    }

    let mut sessions = state.sessions.lock().await;
    let Some(mut session) = sessions.remove(session_id) else {
        return http_error(StatusCode::NOT_FOUND, "unknown MCP session");
    };
    drop(sessions);

    let response = state.core.handle_request(&mut session, request).await;

    let mut sessions = state.sessions.lock().await;
    sessions.insert(session_id.to_string(), session);

    match response {
        Some(result) => http_jsonrpc_response(StatusCode::OK, result, None),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

fn validate_mcp_http_headers(
    headers: &HeaderMap,
    allow_body: bool,
) -> std::result::Result<(), Response> {
    if let Some(origin) = header_value(headers, header::ORIGIN.as_str())
        && !is_loopback_origin(origin) {
            return Err(http_error(
                StatusCode::FORBIDDEN,
                "non-loopback Origin rejected",
            ));
        }

    if !allow_body
        && let Some(method) = headers.get(header::CONTENT_TYPE)
            && method
                .to_str()
                .map(|s| s.starts_with("application/json"))
                .unwrap_or(false)
            {
                return Err(http_error(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "DELETE does not accept a JSON body",
                ));
            }

    Ok(())
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
}

pub(crate) fn is_loopback_origin(origin: &str) -> bool {
    let origin = origin.trim();
    for scheme in ["http://", "https://"] {
        if let Some(rest) = origin.strip_prefix(scheme) {
            let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
            return matches!(
                host,
                "127.0.0.1" | "localhost" | "[::1]" | "127.0.0.1:0" | "localhost:0" | "[::1]:0"
            ) || host.starts_with("127.0.0.1:")
                || host.starts_with("localhost:")
                || host.starts_with("[::1]:");
        }
    }
    false
}

fn http_jsonrpc_error(id: Option<Value>, code: i64, message: &str) -> Response {
    http_jsonrpc_response(StatusCode::OK, jsonrpc_error(id, code, message), None)
}

fn http_jsonrpc_response(
    status: StatusCode,
    body: Value,
    extra_header: Option<(axum::http::HeaderName, String)>,
) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("mcp-protocol-version"),
        HeaderValue::from_static(MCP_PROTOCOL_VERSION),
    );
    if let Some((name, value)) = extra_header
        && let Ok(value) = HeaderValue::from_str(&value) {
            response.headers_mut().insert(name, value);
        }
    response
}

fn http_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [("Content-Type", "text/plain; charset=utf-8")],
        message.to_string(),
    )
        .into_response()
}
