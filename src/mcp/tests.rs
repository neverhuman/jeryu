//! Owner: MCP adapter for external coding agents
//! Proof: `cargo check -p jeryu --message-format=json` and `cargo test -p jeryu --lib mcp`
//! Invariants: These tests guard the adapter behavior and must not mutate policy state.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::json;

use super::{
    MCP_PROTOCOL_VERSION,
    core::{McpCore, McpSessionState},
    http::{McpHttpState, mcp_router},
    tool_manifest,
};

async fn spawn_http_server() -> (String, tokio::task::JoinHandle<()>) {
    let client = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:9", None);
    let state = Arc::new(McpHttpState::new(client));
    let app = mcp_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), server)
}

#[test]
fn manifest_includes_capability_tools() {
    let manifest = tool_manifest();
    assert!(
        manifest
            .iter()
            .any(|tool| tool["name"] == "jeryu.run_tests")
    );
    assert!(
        manifest
            .iter()
            .any(|tool| tool["name"] == "jeryu.fetch_capsule")
    );
    assert!(
        manifest
            .iter()
            .any(|tool| tool["name"] == "jeryu.get_pipeline_jobs")
    );
    assert!(
        manifest
            .iter()
            .any(|tool| tool["name"] == "jeryu.get_ci_bottlenecks")
    );
}

#[test]
fn manifest_covers_all_capability_actions() {
    let manifest = tool_manifest();
    let names: std::collections::BTreeSet<String> = manifest
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(ToString::to_string))
        .collect();

    for entry in crate::tui::action_registry::REGISTRY
        .iter()
        .filter(|entry| {
            entry
                .surfaces
                .contains(&crate::tui::action_registry::Surface::Capability)
        })
    {
        assert!(
            names.contains(&format!("jeryu.{}", entry.id)),
            "missing MCP tool for capability action {}",
            entry.id
        );
    }
}

#[test]
fn loopback_origin_validation_is_strict() {
    assert!(super::http::is_loopback_origin("http://127.0.0.1:8899"));
    assert!(super::http::is_loopback_origin("http://localhost:8899"));
    assert!(super::http::is_loopback_origin("https://[::1]:8899"));
    assert!(!super::http::is_loopback_origin("https://example.com"));
    assert!(!super::http::is_loopback_origin("http://localhost.evil"));
}

#[tokio::test]
async fn stdio_initialize_and_tools_list_work() {
    let client = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:9", None);
    let core = McpCore::new(client);
    let mut state = McpSessionState::new();

    let init = core
        .handle_line(
            &mut state,
            &serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "clientInfo": { "name": "test", "version": "0.1.0" }
                }
            }))
            .unwrap(),
        )
        .await;
    assert_eq!(init.len(), 1);
    assert_eq!(init[0]["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);

    let list = core
        .handle_line(
            &mut state,
            &serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {}
            }))
            .unwrap(),
        )
        .await;
    assert_eq!(list.len(), 1);
    assert!(list[0]["result"]["tools"].is_array());
}

#[tokio::test]
async fn http_transport_initializes_and_executes_tools() {
    let (base, server) = spawn_http_server().await;
    let client = reqwest::Client::new();

    let origin = base.clone();

    let init_resp = client
        .post(format!("{base}/mcp"))
        .header("Origin", &origin)
        .header("Mcp-Method", "initialize")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "clientInfo": { "name": "reqwest", "version": "0.1.0" }
            }
        }))
        .send()
        .await
        .unwrap();
    assert!(init_resp.status().is_success());
    let session = init_resp
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();

    let list_resp = client
        .post(format!("{base}/mcp"))
        .header("Origin", &origin)
        .header("Mcp-Session-Id", &session)
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .header("Mcp-Method", "tools/list")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .unwrap();
    assert!(list_resp.status().is_success());
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    assert!(list_body["result"]["tools"].is_array());

    let call_resp = client
        .post(format!("{base}/mcp"))
        .header("Origin", &origin)
        .header("Mcp-Session-Id", &session)
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .header("Mcp-Method", "tools/call")
        .header("Mcp-Name", "jeryu.explain_blockers")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "jeryu.explain_blockers",
                "arguments": { "entity_type": "merge", "entity_id": 1 }
            }
        }))
        .send()
        .await
        .unwrap();
    assert!(call_resp.status().is_success());
    let call_body: serde_json::Value = call_resp.json().await.unwrap();
    assert!(call_body["result"]["content"].is_array());

    let delete_resp = client
        .delete(format!("{base}/mcp"))
        .header("Origin", &origin)
        .header("Mcp-Session-Id", &session)
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .send()
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

    server.abort();
}

#[tokio::test]
async fn http_transport_rejects_malformed_json() {
    let (base, server) = spawn_http_server().await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("Origin", &base)
        .header("Mcp-Method", "initialize")
        .body("not-json")
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32700);

    server.abort();
}

#[tokio::test]
async fn http_transport_rejects_unknown_tools() {
    let (base, server) = spawn_http_server().await;
    let client = reqwest::Client::new();

    let origin = base.clone();

    let init_resp = client
        .post(format!("{base}/mcp"))
        .header("Origin", &origin)
        .header("Mcp-Method", "initialize")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION
            }
        }))
        .send()
        .await
        .unwrap();
    let session = init_resp
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();

    let resp = client
        .post(format!("{base}/mcp"))
        .header("Origin", &origin)
        .header("Mcp-Session-Id", &session)
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .header("Mcp-Method", "tools/call")
        .header("Mcp-Name", "jeryu.not_a_real_tool")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "jeryu.not_a_real_tool",
                "arguments": {}
            }
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);

    server.abort();
}

#[tokio::test]
async fn http_transport_rejects_non_loopback_origins() {
    let (base, server) = spawn_http_server().await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("Origin", "https://example.com")
        .header("Mcp-Method", "initialize")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    server.abort();
}

#[tokio::test]
async fn http_transport_rejects_unknown_sessions() {
    let (base, server) = spawn_http_server().await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("Origin", &base)
        .header("Mcp-Session-Id", "missing")
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .header("Mcp-Method", "tools/list")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    server.abort();
}

#[tokio::test]
async fn http_get_is_not_enabled() {
    let (base, server) = spawn_http_server().await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/mcp"))
        .header("Origin", &base)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);

    server.abort();
}
