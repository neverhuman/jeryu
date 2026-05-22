use serde_json::Value;

use super::{
    MCP_PROTOCOL_VERSION, McpCore, McpSessionState, TOOL_PREFIX, ensure_initialized, jsonrpc_error,
    jsonrpc_result,
};

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
    let call: super::CallToolRequestParams = match serde_json::from_value(params) {
        Ok(value) => value,
        Err(err) => {
            return jsonrpc_error(
                Some(id),
                -32602,
                &format!("invalid tools/call params: {err}"),
            );
        }
    };

    let Some(tool) =
        super::super::tools::tool_definition(call.name.trim_start_matches(TOOL_PREFIX))
    else {
        return jsonrpc_error(Some(id), -32601, &format!("unknown tool: {}", call.name));
    };

    let Some(intent) = tool.build_intent(call.arguments.unwrap_or(Value::Null)) else {
        return jsonrpc_error(Some(id), -32602, "invalid tool arguments");
    };

    let ctx = crate::capability::CapabilityContext::mcp(
        format!("mcp-{}", id),
        state.client_actor.clone(),
        MCP_PROTOCOL_VERSION.to_string(),
    );
    let response = crate::capability::execute_intent(intent, &ctx, &core.client).await;
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
