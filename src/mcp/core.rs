//! Owner: MCP adapter for external coding agents
//! Proof: `cargo check -p jeryu --message-format=json` and `cargo test -p jeryu --lib mcp`
//! Invariants: MCP request handling must remain a thin adapter over the capability policy;
//!             it must not bypass grant checks, evidence handling, or merge/release gates.

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use super::{MCP_PROTOCOL_VERSION, TOOL_PREFIX};

#[derive(Debug, Clone, Deserialize)]
struct Implementation {
    name: String,
    version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct InitializeRequestParams {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    #[serde(rename = "clientCapabilities", default)]
    _client_capabilities: Option<Value>,
    #[serde(rename = "clientInfo")]
    client_info: Option<Implementation>,
}

#[derive(Debug, Clone, Deserialize)]
struct ListToolsRequestParams {
    #[serde(default)]
    #[serde(rename = "cursor")]
    _cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CallToolRequestParams {
    name: String,
    #[serde(default, rename = "arguments")]
    arguments: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct JsonRpcRequest {
    pub(crate) jsonrpc: String,
    #[serde(default)]
    pub(crate) id: Option<Value>,
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) params: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum IncomingMessage {
    Request(JsonRpcRequest),
    Batch(Vec<JsonRpcRequest>),
    Raw(Value),
}

pub(crate) struct McpSessionState {
    initialized: bool,
    client_actor: String,
}

impl McpSessionState {
    pub(crate) fn new() -> Self {
        Self {
            initialized: false,
            client_actor: "mcp-client".to_string(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct McpCore {
    client: crate::gitlab_client::GitlabClient,
}

impl McpCore {
    pub(crate) fn new(client: crate::gitlab_client::GitlabClient) -> Self {
        Self { client }
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
                && let Some(Value::String(description)) = map.get("description") {
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
        state.client_actor = req
            .client_info
            .as_ref()
            .map(|info| {
                let version = info.version.as_deref().unwrap_or("unknown");
                format!("mcp:{}:{version}", info.name)
            })
            .unwrap_or_else(|| "mcp-client".to_string());

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
            serde_json::json!({ "tools": super::tools::tool_manifest() }),
        )
    }

    async fn handle_tools_call(
        &self,
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

        let Some(tool) = super::tools::tool_definition(call.name.trim_start_matches(TOOL_PREFIX))
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
        let response = crate::capability::execute_intent(intent, &ctx, &self.client).await;
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
}

pub async fn start_mcp_stdio(client: crate::gitlab_client::GitlabClient) -> Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = BufWriter::new(tokio::io::stdout());
    let mut lines = stdin.lines();
    let core = McpCore::new(client);
    let mut state = McpSessionState::new();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let responses = core.handle_line(&mut state, line).await;
        if responses.is_empty() {
            continue;
        }

        let payload = if responses.len() == 1 {
            serde_json::to_vec(&responses[0])?
        } else {
            serde_json::to_vec(&responses)?
        };
        stdout.write_all(&payload).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

fn ensure_initialized(state: &McpSessionState) -> Result<()> {
    if state.initialized {
        Ok(())
    } else {
        bail!("server not initialized")
    }
}

pub(crate) fn jsonrpc_result(id: Value, result: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

pub(crate) fn jsonrpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    let mut obj = serde_json::json!({
        "jsonrpc": "2.0",
        "error": {
            "code": code,
            "message": message,
        }
    });
    if let Some(id) = id {
        obj.as_object_mut()
            .expect("json object")
            .insert("id".to_string(), id);
    }
    obj
}
