//! Owner: MCP adapter for external coding agents
//! Proof: `cargo check -p jeryu --message-format=json` and `cargo test -p jeryu --lib mcp`
//! Invariants: MCP request handling must remain a thin adapter over the capability policy;
//!             it must not bypass grant checks, evidence handling, or merge/release gates.

use super::{MCP_PROTOCOL_VERSION, TOOL_PREFIX};

#[path = "core_protocol.rs"]
mod core_protocol;

pub(crate) use core_protocol::{
    CallToolRequestParams, IncomingMessage, InitializeRequestParams, JsonRpcRequest,
    ListToolsRequestParams,
};

#[path = "core_tools.rs"]
mod core_tools;

#[path = "core_dispatch.rs"]
mod core_dispatch;

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
}

// ---------------------------------------------------------------------------
// stdio server + JSON-RPC helpers (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "core_io.rs"]
mod core_io;
pub use core_io::*;

#[path = "core_jsonrpc.rs"]
mod core_jsonrpc;
pub(crate) use core_jsonrpc::{ensure_initialized, jsonrpc_error, jsonrpc_result};
