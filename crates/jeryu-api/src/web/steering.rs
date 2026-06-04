use axum::Json;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, Method as HttpMethod, header};
use axum::middleware::Next;
use axum::response::Response as AxumResponse;
use serde_json::{Value, json};

use crate::github::{MCP_GUIDANCE_TOOLS, MCP_RUN_TESTS_TOOL};

pub(super) const MCP_READ_TOOL: &str = "jeryu.get_system_snapshot";
pub(super) const MCP_CHECKS_TOOL: &str = "jeryu.get_ci_run_jobs";
pub(super) const MCP_BLOCKERS_TOOL: &str = "jeryu.explain_blockers";
pub(super) const MCP_PATCH_TOOL: &str = "jeryu.propose_patch";
pub(super) const MCP_MERGE_TOOL: &str = "jeryu.request_merge";
pub(super) const MCP_ISSUE_TOOL: &str = "jeryu.bug_submit";

pub(super) const HDR_API: &str = "x-jeryu-api";
pub(super) const HDR_FAST_PATH: &str = "x-jeryu-fast-path";
pub(super) const HDR_TOOL: &str = "x-jeryu-tool";

/// Response middleware: stamps every reply with advisory steering headers.
pub(super) async fn steer_headers(request: Request, next: Next) -> AxumResponse {
    let user_agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in advisory_headers(&user_agent, &method, &path) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            headers.insert(name, value);
        }
    }
    response
}

/// Pure builder for advisory steering headers.
pub(super) fn advisory_headers(
    user_agent: &str,
    method: &HttpMethod,
    path: &str,
) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        (HDR_API, "v4".to_string()),
        (HDR_FAST_PATH, "/.jeryu/capabilities".to_string()),
    ];
    if is_automation_agent(user_agent)
        && let Some(tool) = suggested_tool(method, path)
    {
        headers.push((HDR_TOOL, tool.to_string()));
    }
    headers
}

/// Heuristic for `gh`, generic HTTP automation, or Jeryu/agent user-agents.
pub(super) fn is_automation_agent(user_agent: &str) -> bool {
    let ua = user_agent.to_ascii_lowercase();
    const NEEDLES: [&str; 7] = [
        "github cli",
        "go-gh",
        "okhttp",
        "curl",
        "python-requests",
        "jeryu",
        "agent",
    ];
    NEEDLES.iter().any(|needle| ua.contains(needle))
}

/// Suggests the Jeryu MCP tool for a route+method.
pub(super) fn suggested_tool(method: &HttpMethod, path: &str) -> Option<&'static str> {
    let trimmed = path.trim_end_matches('/');
    match *method {
        HttpMethod::POST if trimmed.ends_with("/pulls") => Some(MCP_PATCH_TOOL),
        HttpMethod::POST if trimmed.contains("/actions/") => Some(MCP_RUN_TESTS_TOOL),
        HttpMethod::PUT if trimmed.ends_with("/merge") => Some(MCP_MERGE_TOOL),
        HttpMethod::POST if trimmed.ends_with("/issues") => Some(MCP_ISSUE_TOOL),
        HttpMethod::GET if trimmed.contains("/actions/") => Some(MCP_CHECKS_TOOL),
        HttpMethod::GET if trimmed.contains("/check-runs") => Some(MCP_CHECKS_TOOL),
        HttpMethod::GET if trimmed.contains("/pulls") => Some(MCP_BLOCKERS_TOOL),
        HttpMethod::GET => Some(MCP_READ_TOOL),
        _ => None,
    }
}

/// Capability manifest for external agents and `gh` users.
pub(super) async fn capabilities() -> Json<Value> {
    Json(capabilities_payload())
}

/// Pure builder for the `/.jeryu/capabilities` payload.
pub(super) fn capabilities_payload() -> Value {
    json!({
        "server": "jeryu",
        "api_version": "v4",
        "graphql": "/graphql",
        "websocket": "/api/v1/ws",
        "mcp_endpoint": "/mcp",
        "mcp_tools": MCP_GUIDANCE_TOOLS,
        "gh_command_map": {
            "gh pr create": MCP_PATCH_TOOL,
            "gh pr merge": MCP_MERGE_TOOL,
            "gh pr list": "GET /repos/{owner}/{repo}/pulls",
            "gh workflow list": "GET /repos/{owner}/{repo}/actions/workflows",
            "gh workflow view": "GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}",
            "gh run list": "GET /repos/{owner}/{repo}/actions/runs",
            "gh run view": "GET /repos/{owner}/{repo}/actions/runs/{id}",
            "gh workflow run": MCP_RUN_TESTS_TOOL,
            "gh run rerun": MCP_RUN_TESTS_TOOL,
            "gh run cancel": MCP_RUN_TESTS_TOOL,
            "gh issue create": MCP_ISSUE_TOOL,
            "gh api": "Use /.jeryu/capabilities and the listed jeryu.* MCP tools; unsupported REST returns guided JSON.",
            "gh repo create": "POST /repos",
        },
        "fast_path_advice":
            "Prefer the jeryu MCP tools for mutations; gh REST/GraphQL is supported but slower.",
    })
}
