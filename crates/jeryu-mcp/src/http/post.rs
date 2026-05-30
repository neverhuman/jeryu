//! `POST /mcp` handler (port of `http_post.rs`): header validation, session minting,
//! `Mcp-Method`/`Mcp-Name` header binding. No batch over HTTP.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use uuid::Uuid;

use super::McpHttpState;
use super::support::{
    header_value, http_error, http_jsonrpc_error, http_jsonrpc_response, validate_mcp_http_headers,
};
use crate::MCP_PROTOCOL_VERSION;
use crate::core::{JsonRpcRequest, McpSessionState};

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
            && name != name_header
        {
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
