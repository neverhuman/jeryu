//! `tools/call` handler (port of `core_tools.rs`), repointed from `capability::execute_intent`
//! onto the `ToolBackend` seam.

use serde_json::Value;

use super::{
    CallToolRequestParams, McpCore, McpSessionState, ensure_initialized, jsonrpc_error,
    jsonrpc_result,
};
use crate::backend::McpCallContext;
use crate::tools::tool_definition;
use crate::{MCP_PROTOCOL_VERSION, TOOL_PREFIX};

pub(crate) async fn handle_tools_call(
    core: &McpCore,
    state: &mut McpSessionState,
    id: Value,
    params: Option<Value>,
) -> Value {
    if let Err(err) = ensure_initialized(state) {
        return jsonrpc_error(Some(id), -32002, &err.to_string());
    }

    let params = match params {
        Some(value) => value,
        None => return jsonrpc_error(Some(id), -32602, "tools/call params are required"),
    };
    let call: CallToolRequestParams = match serde_json::from_value(params) {
        Ok(value) => value,
        Err(err) => {
            return jsonrpc_error(
                Some(id),
                -32602,
                &format!("invalid tools/call params: {err}"),
            );
        }
    };

    let action_id = call.name.trim_start_matches(TOOL_PREFIX);
    let Some(tool) = tool_definition(action_id) else {
        return jsonrpc_error(Some(id), -32601, &format!("unknown tool: {}", call.name));
    };

    let Some(args) = tool.normalize_args(call.arguments.unwrap_or(Value::Null)) else {
        return jsonrpc_error(Some(id), -32602, "invalid tool arguments");
    };

    let ctx = McpCallContext::mcp(
        format!("mcp-{id}"),
        state.client_actor.clone(),
        MCP_PROTOCOL_VERSION.to_string(),
    );

    let response = match core.backend().call(action_id, args, &ctx) {
        Ok(response) => response,
        Err(err) => crate::backend::ToolResponse::error(err.to_string()),
    };
    let is_error = !response.success;
    let message = response.message.clone();

    jsonrpc_result(
        id,
        serde_json::json!({
            "content": [ { "type": "text", "text": message } ],
            "structuredContent": response,
            "isError": is_error,
        }),
    )
}
