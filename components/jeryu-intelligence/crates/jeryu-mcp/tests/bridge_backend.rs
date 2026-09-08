//! End-to-end: the real jeryu-agentbridge-backed `BridgeBackend` driven through
//! the MCP `tools/call` transport path.
//!
//! These assert the contract the slice requires: `propose_patch` performs a real
//! scope-validated bounded mutation through agentbridge (recording an audit
//! receipt) and is DENIED when the write escapes the agent's allowed scope, and
//! `request_merge` evaluates mergeability through the same proof gate.

use std::sync::Arc;

use serde_json::{Value, json};

use jeryu_mcp::{BridgeBackend, MCP_PROTOCOL_VERSION, McpCore, McpSessionState};

async fn initialized_core(backend: BridgeBackend) -> (McpCore, McpSessionState) {
    let core = McpCore::new(Arc::new(backend));
    let mut state = McpSessionState::new();
    core.handle_line_test(
        &mut state,
        &serde_json::to_string(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": MCP_PROTOCOL_VERSION }
        }))
        .unwrap(),
    )
    .await;
    (core, state)
}

async fn call(core: &McpCore, state: &mut McpSessionState, name: &str, args: Value) -> Value {
    let resp = core
        .handle_line_test(
            state,
            &serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": { "name": name, "arguments": args }
            }))
            .unwrap(),
        )
        .await;
    assert_eq!(resp.len(), 1, "expected exactly one response");
    resp.into_iter().next().unwrap()
}

fn propose_args(file_path: &str) -> Value {
    json!({
        "repo": 42,
        "branch_name": "agent/fix",
        "base_ref": "main",
        "commit_message": "scoped fix",
        "modifications": [ { "file_path": file_path, "content": "x" } ],
    })
}

#[tokio::test]
async fn propose_patch_in_scope_runs_real_scoped_mutation() {
    let backend = BridgeBackend::with_allowed_paths(vec!["crates/jeryu_agentbridge/".to_string()]);
    let (core, mut state) = initialized_core(backend).await;

    let resp = call(
        &core,
        &mut state,
        "jeryu.propose_patch",
        propose_args("crates/jeryu_agentbridge/src/api.rs"),
    )
    .await;

    assert_eq!(resp["result"]["isError"], json!(false));
    let structured = &resp["result"]["structuredContent"];
    assert_eq!(structured["success"], json!(true));
    let receipt = structured["data"]["receipt_id"]
        .as_str()
        .expect("receipt id present");
    assert!(
        receipt.starts_with("receipt"),
        "expected a real agentbridge receipt id, got {receipt}"
    );
    assert_eq!(
        structured["data"]["changed_paths"],
        json!(["crates/jeryu_agentbridge/src/api.rs"])
    );
}

#[tokio::test]
async fn propose_patch_out_of_scope_is_denied() {
    let backend = BridgeBackend::with_allowed_paths(vec!["crates/jeryu_agentbridge/".to_string()]);
    let (core, mut state) = initialized_core(backend).await;

    let resp = call(
        &core,
        &mut state,
        "jeryu.propose_patch",
        propose_args("crates/jeryu_proof/src/engine.rs"),
    )
    .await;

    assert_eq!(resp["result"]["isError"], json!(true));
    let structured = &resp["result"]["structuredContent"];
    assert_eq!(structured["success"], json!(false));
    assert!(
        structured["message"]
            .as_str()
            .unwrap()
            .contains("out-of-scope"),
        "expected scope denial, got {}",
        structured["message"]
    );
    assert!(structured.get("data").is_none() || structured["data"].is_null());
}

#[tokio::test]
async fn request_merge_evaluates_through_proof_gate() {
    let backend = BridgeBackend::with_allowed_paths(vec!["crates/jeryu_agentbridge/".to_string()]);
    let (core, mut state) = initialized_core(backend).await;

    // Propose first so the PR is present in the bridge.
    call(
        &core,
        &mut state,
        "jeryu.propose_patch",
        propose_args("crates/jeryu_agentbridge/src/api.rs"),
    )
    .await;

    let resp = call(
        &core,
        &mut state,
        "jeryu.request_merge",
        json!({
            "repo": 42,
            "pr_number": 1,
            "source_branch": "agent/fix",
            "target_branch": "main",
        }),
    )
    .await;

    let structured = &resp["result"]["structuredContent"];
    assert_eq!(structured["success"], json!(true));
    // A freshly proposed PR has no proof witness yet, so the gate reports it as
    // not-yet-mergeable with real blockers — proving a live evaluation, not a no-op.
    assert_eq!(structured["data"]["mergeable"], json!(false));
    assert!(
        structured["data"]["blockers"]
            .as_array()
            .is_some_and(|b| !b.is_empty())
    );
}
