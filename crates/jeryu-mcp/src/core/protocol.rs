//! JSON-RPC / MCP request serde types (verbatim port of `core_protocol.rs`).

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Implementation {
    pub(crate) name: String,
    pub(crate) version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InitializeRequestParams {
    #[serde(rename = "protocolVersion")]
    pub(crate) protocol_version: String,
    #[serde(rename = "clientCapabilities", default)]
    pub(crate) _client_capabilities: Option<Value>,
    #[serde(rename = "clientInfo")]
    pub(crate) client_info: Option<Implementation>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ListToolsRequestParams {
    #[serde(default)]
    #[serde(rename = "cursor")]
    pub(crate) _cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CallToolRequestParams {
    pub(crate) name: String,
    #[serde(default, rename = "arguments")]
    pub(crate) arguments: Option<Value>,
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
pub(crate) enum IncomingMessage {
    Request(JsonRpcRequest),
    Batch(Vec<JsonRpcRequest>),
    Raw(Value),
}
