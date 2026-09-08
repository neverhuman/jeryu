//! MCP core: session state, the backend-holding `McpCore`, the JSON-RPC dispatch
//! (`initialize` / `ping` / `tools/list` / `tools/call`), and the stdio server loop.

use std::sync::Arc;

use serde_json::Value;

use crate::MCP_PROTOCOL_VERSION;
use crate::backend::ToolBackend;

mod io;
mod jsonrpc;
mod protocol;
mod tools_call;

pub use io::start_mcp_stdio;

pub(crate) use jsonrpc::{ensure_initialized, jsonrpc_error, jsonrpc_result};
pub(crate) use protocol::{
    CallToolRequestParams, IncomingMessage, InitializeRequestParams, JsonRpcRequest,
    ListToolsRequestParams,
};

/// Per-session state for an MCP connection.
pub struct McpSessionState {
    pub(crate) initialized: bool,
    pub(crate) client_actor: String,
}

impl Default for McpSessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl McpSessionState {
    pub fn new() -> Self {
        Self {
            initialized: false,
            client_actor: "mcp-client".to_string(),
        }
    }
}

/// The MCP core. Cheaply cloneable: it holds an `Arc<dyn ToolBackend>` (the seam onto
/// the future jeryu-* engine) rather than a concrete client, so it can be shared across
/// concurrent HTTP sessions.
#[derive(Clone)]
pub struct McpCore {
    backend: Arc<dyn ToolBackend>,
}

impl McpCore {
    pub fn new(backend: Arc<dyn ToolBackend>) -> Self {
        Self { backend }
    }

    pub(crate) fn backend(&self) -> &Arc<dyn ToolBackend> {
        &self.backend
    }

    /// Drive one JSON-RPC line through the dispatcher and return the response(s).
    /// Public so integration tests can exercise the stdio path without a real stream.
    pub async fn handle_line_test(&self, state: &mut McpSessionState, line: &str) -> Vec<Value> {
        self.handle_line(state, line).await
    }

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
                "instructions": "Use tools/list to discover the jeryu tool surface. Each tool executes through the same policy, grant, and evidence gates as the engine backend."
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

        let tools: Vec<Value> = self
            .backend
            .list()
            .iter()
            .map(|d| d.to_mcp_json())
            .collect();
        jsonrpc_result(id, serde_json::json!({ "tools": tools }))
    }

    async fn handle_tools_call(
        &self,
        state: &mut McpSessionState,
        id: Value,
        params: Option<Value>,
    ) -> Value {
        tools_call::handle_tools_call(self, state, id, params).await
    }
}
