//! Loopback/Origin enforcement + JSON-RPC-over-HTTP envelope helpers
//! (near-verbatim port of `http_support.rs`).

use axum::{
    Json,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::Value;

use crate::MCP_PROTOCOL_VERSION;
use crate::core::jsonrpc_error;

#[allow(clippy::result_large_err)] // Response is the unified MCP rejection type.
pub(crate) fn validate_mcp_http_headers(
    headers: &HeaderMap,
    allow_body: bool,
) -> std::result::Result<(), Response> {
    if let Some(origin) = header_value(headers, header::ORIGIN.as_str())
        && !is_loopback_origin(origin)
    {
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
            "JSON body not accepted for this method",
        ));
    }

    Ok(())
}

pub(crate) fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
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

pub(crate) fn http_jsonrpc_error(id: Option<Value>, code: i64, message: &str) -> Response {
    http_jsonrpc_response(StatusCode::OK, jsonrpc_error(id, code, message), None)
}

pub(crate) fn http_jsonrpc_response(
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
        && let Ok(value) = HeaderValue::from_str(&value)
    {
        response.headers_mut().insert(name, value);
    }
    response
}

pub(crate) fn http_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [("Content-Type", "text/plain; charset=utf-8")],
        message.to_string(),
    )
        .into_response()
}
