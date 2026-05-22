use super::*;
use serde_json::Value;

impl McpCore {
    pub(crate) async fn handle_line(&self, state: &mut McpSessionState, line: &str) -> Vec<Value> {
        let parsed = match serde_json::from_str::<IncomingMessage>(line) {
            Ok(message) => message,
            Err(err) => return vec![jsonrpc_error(None, -32700, &format!("parse error: {err}"))],
        };

        match parsed {
            IncomingMessage::Request(request) => match self.handle_request(state, request).await {
                Some(response) => vec![response],
                None => vec![],
            },
            IncomingMessage::Batch(requests) => {
                let mut responses = Vec::new();
                for request in requests {
                    if let Some(response) = self.handle_request(state, request).await {
                        responses.push(response);
                    }
                }
                responses
            }
            IncomingMessage::Raw(value) => match value {
                Value::Object(_) => vec![jsonrpc_error(None, -32600, "invalid request")],
                Value::Array(_) => vec![jsonrpc_error(None, -32600, "invalid request batch")],
                _ => vec![jsonrpc_error(None, -32700, "parse error")],
            },
        }
    }

    pub(crate) async fn handle_request(
        &self,
        state: &mut McpSessionState,
        request: JsonRpcRequest,
    ) -> Option<Value> {
        if request.jsonrpc != "2.0" {
            return Some(jsonrpc_error(request.id, -32600, "invalid jsonrpc version"));
        }

        if request.method.starts_with("notifications/") && request.id.is_none() {
            self.handle_notification(state, &request.method, request.params)
                .await;
            return None;
        }

        let Some(id) = request.id else {
            return Some(jsonrpc_error(None, -32600, "request id is required"));
        };

        match request.method.as_str() {
            "initialize" => Some(self.handle_initialize(state, id, request.params).await),
            "ping" => Some(jsonrpc_result(id, serde_json::json!({}))),
            "tools/list" => Some(self.handle_tools_list(state, id, request.params).await),
            "tools/call" => Some(self.handle_tools_call(state, id, request.params).await),
            other => Some(jsonrpc_error(
                Some(id),
                -32601,
                &format!("method not found: {other}"),
            )),
        }
    }

    async fn handle_notification(
        &self,
        state: &mut McpSessionState,
        method: &str,
        params: Option<Value>,
    ) {
        if method == "notifications/initialized" {
            state.initialized = true;
            if let Some(Value::Object(map)) = params
                && let Some(Value::String(description)) = map.get("description")
            {
                state.client_actor = description.clone();
            }
        }
    }

    async fn handle_initialize(
        &self,
        state: &mut McpSessionState,
        id: Value,
        params: Option<Value>,
    ) -> Value {
        let params = match params {
            Some(value) => value,
            None => return jsonrpc_error(Some(id), -32602, "initialize params are required"),
        };
        let req: InitializeRequestParams = match serde_json::from_value(params) {
            Ok(value) => value,
            Err(err) => {
                return jsonrpc_error(
                    Some(id),
                    -32602,
                    &format!("invalid initialize params: {err}"),
                );
            }
        };
        if req.protocol_version != MCP_PROTOCOL_VERSION {
            return jsonrpc_error(
                Some(id),
                -32602,
                &format!(
                    "unsupported protocolVersion '{}', expected '{}'",
                    req.protocol_version, MCP_PROTOCOL_VERSION
                ),
            );
        }

        state.initialized = true;
        state.client_actor = match req.client_info.as_ref() {
            Some(info) => {
                let version = info.version.as_deref().unwrap_or("unknown");
                format!("mcp:{}:{version}", info.name)
            }
            None => "mcp-client".to_string(),
        };

        jsonrpc_result(
            id,
            serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "jeryu",
                    "version": env!("CARGO_PKG_VERSION"),
                    "description": "MCP adapter over jeryu capability policy"
                },
                "instructions": "Use tools/list to discover the jeryu tool surface. Each tool executes through the same policy, grant, and evidence gates as the capability socket."
            }),
        )
    }

    async fn handle_tools_list(
        &self,
        state: &mut McpSessionState,
        id: Value,
        params: Option<Value>,
    ) -> Value {
        if let Err(err) = ensure_initialized(state) {
            return jsonrpc_error(Some(id), -32002, &err.to_string());
        }

        if let Some(params) = params {
            let _: ListToolsRequestParams = match serde_json::from_value(params) {
                Ok(value) => value,
                Err(err) => {
                    return jsonrpc_error(
                        Some(id),
                        -32602,
                        &format!("invalid tools/list params: {err}"),
                    );
                }
            };
        }

        jsonrpc_result(
            id,
            serde_json::json!({ "tools": crate::mcp::tools::tool_manifest() }),
        )
    }

    async fn handle_tools_call(
        &self,
        state: &mut McpSessionState,
        id: Value,
        params: Option<Value>,
    ) -> Value {
        core_tools::handle_tools_call(self, state, id, params).await
    }
}
