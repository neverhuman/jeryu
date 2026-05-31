//! Shared request parsing, principal resolution, and response helpers for the
//! GitHub-compatible edge.
//!
//! These helpers are used across every resource submodule so they live in one
//! place; the GitHub-shaped status codes (422 for unparseable/invalid bodies,
//! 405 for branch-protection blocks, 404 for misses) are asserted by the
//! `github_api` conformance tests and must stay byte-for-byte.

use jeryu_core::ForgeError;
use serde_json::{Value, json};

use crate::routes::Response;

pub(super) const MCP_GUIDANCE_TOOLS: &[&str] = &[
    "jeryu.get_system_snapshot",
    "jeryu.get_ci_run_jobs",
    "jeryu.explain_blockers",
    "jeryu.plan_validation",
    "jeryu.propose_patch",
    "jeryu.request_merge",
    "jeryu.bug_submit",
    "jeryu.bug_list",
];

pub(super) fn parse_body<T: serde::de::DeserializeOwned>(
    body: &str,
) -> std::result::Result<T, Response> {
    serde_json::from_str(body).map_err(|err| {
        // GitHub returns 422 Unprocessable Entity for a body it cannot parse
        // or that fails validation.
        json_response(
            422,
            &json!({
                "message": "Validation Failed",
                "errors": [{ "code": "invalid", "detail": err.to_string() }],
                "documentation_url": docs_url(),
            }),
        )
    })
}

pub(super) fn parse_number(raw: &str) -> std::result::Result<u64, Response> {
    raw.parse::<u64>().map_err(|_| {
        json_response(
            422,
            &json!({
                "message": "Validation Failed",
                "errors": [{ "field": "number", "code": "invalid" }],
                "documentation_url": docs_url(),
            }),
        )
    })
}

/// Resolves the acting principal from the request body's optional `actor`
/// field, defaulting to the canonical service principal. The future HTTP edge
/// will replace this with the authenticated token's owner.
pub(super) fn actor(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("actor")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "jeryu".to_owned())
}

/// Resolves the owner for repo creation from the request body's optional
/// `owner` field.
pub(super) fn owner_for_create(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value
            .get("owner")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

/// Renders an `owner`/`user` actor block shared by every GitHub-shaped entity.
pub(super) fn owner_json(login: &str) -> Value {
    json!({
        "login": login,
        "type": "User",
        "url": format!("/users/{login}"),
    })
}

pub(super) fn json_response(status: u16, value: &Value) -> Response {
    Response {
        status,
        body: value.to_string(),
    }
}

pub(super) fn error_response(err: ForgeError) -> Response {
    let status = match err {
        ForgeError::NotFound(_) => 404,
        ForgeError::Conflict(_) => 422,
        ForgeError::Validation(_) => 422,
        ForgeError::BranchProtection(_) => 405,
        ForgeError::Storage(_) => 500,
    };
    json_response(
        status,
        &json!({ "message": err.to_string(), "documentation_url": docs_url() }),
    )
}

pub(super) fn not_found(status: u16) -> Response {
    json_response(
        status,
        &json!({
            "message": "Not Found",
            "documentation_url": docs_url(),
            "jeryu_repair_hint": {
                "purpose": "route unsupported GitHub-compatible REST request",
                "reason": "the request path is outside the current guided Jeryu GitHub subset",
                "common_fixes": [
                    "retry with one of the listed GitHub-compatible REST routes",
                    "use the typed Jeryu MCP/API tool for repository, PR, issue, check, release, or hook workflows",
                    "add a conformance test before widening the compatibility subset"
                ],
                "docs_url": docs_url(),
                "repair_hint": "map the command to the closest listed Jeryu route or MCP tool, then rerun cargo test -p jeryu-api --features web"
            },
            "jeryu_mcp_tools": MCP_GUIDANCE_TOOLS,
            "jeryu_api_routes": [
                "GET /user",
                "GET /repos",
                "GET /repos/{owner}/{repo}",
                "GET /repos/{owner}/{repo}/pulls",
                "GET /repos/{owner}/{repo}/issues",
                "GET /repos/{owner}/{repo}/commits/{ref}/status",
                "GET /repos/{owner}/{repo}/commits/{ref}/check-runs",
                "POST /graphql"
            ]
        }),
    )
}

pub(super) fn docs_url() -> String {
    "/docs/rest".to_owned()
}
