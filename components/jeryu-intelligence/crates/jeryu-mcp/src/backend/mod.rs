//! Backend trait seam + in-memory implementation.
//!
//! The MCP transport is a thin adapter. All tool DISPATCH goes behind the
//! [`ToolBackend`] trait so the transport stays free of any one engine choice.
//! [`BridgeBackend`] is the real implementation: the mutating tools route
//! through [`jeryu_agentbridge::AgentBridge`] for scope-validated bounded
//! mutations. [`MemoryBackend`] is kept as a deterministic in-memory test double.
//!
//! Bug tracking (the `bug_*` tools) is split out behind [`BugStore`] so the KEPT
//! RedlineDB persistence layer can be supplied independently of the agent backend.

mod bridge;
mod memory;

pub use bridge::BridgeBackend;
pub use memory::MemoryBackend;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Descriptor for one tool in the catalog. Mirrors the MCP `tools/list` entry shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDescriptor {
    /// Fully-qualified tool name, e.g. `jeryu.fetch_capsule`.
    pub name: String,
    /// Human-readable title.
    pub title: String,
    /// One-line description.
    pub description: String,
    /// JSON Schema for the tool's arguments.
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    /// JSON Schema for the tool's structured result.
    #[serde(rename = "outputSchema")]
    pub output_schema: Value,
    /// MCP behavioral hints (`readOnlyHint`, `destructiveHint`, ...).
    pub annotations: Value,
}

impl ToolDescriptor {
    /// Render to the JSON shape `tools/list` returns.
    pub fn to_mcp_json(&self) -> Value {
        serde_json::json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema,
            "outputSchema": self.output_schema,
            "annotations": self.annotations,
        })
    }
}

/// Result of a tool call. Keeps the historic `{ success, message, data }` JSON shape so
/// the `content` / `structuredContent` / `isError` wrapping in the transport ports
/// unchanged and existing MCP clients see no contract drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub success: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ToolResponse {
    pub fn ok(message: impl Into<String>, data: Value) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

/// Context threaded into each tool call: who is calling (actor), the JSON-RPC request id,
/// and the negotiated protocol version. Feeds agent identity/scope in the real backend.
#[derive(Debug, Clone)]
pub struct McpCallContext {
    pub request_id: String,
    pub actor: String,
    pub protocol_version: String,
}

impl McpCallContext {
    pub fn mcp(
        request_id: impl Into<String>,
        actor: impl Into<String>,
        protocol_version: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            actor: actor.into(),
            protocol_version: protocol_version.into(),
        }
    }
}

/// The single dispatch seam. [`BridgeBackend`] is the real jeryu-agentbridge-backed
/// implementation; [`MemoryBackend`] is the test double.
///
/// `call` receives the *unprefixed* tool id (e.g. `fetch_capsule`, not `jeryu.fetch_capsule`)
/// and the raw JSON arguments object. `list` produces the catalog the transport advertises.
pub trait ToolBackend: Send + Sync {
    /// Dispatch a tool by its unprefixed id with raw JSON arguments.
    fn call(&self, tool: &str, args: Value, ctx: &McpCallContext) -> anyhow::Result<ToolResponse>;

    /// The catalog this backend exposes (already fully-prefixed names).
    fn list(&self) -> Vec<ToolDescriptor>;
}

/// Persistence seam for the `bug_*` tools (KEPT RedlineDB layer in the fused tree).
/// Split from [`ToolBackend`] so the durable bug store can be supplied independently.
pub trait BugStore: Send + Sync {
    fn submit(&self, report: Value, idempotency_key: Option<String>) -> anyhow::Result<Value>;
    fn list(
        &self,
        project: Option<String>,
        status: Option<String>,
        sort: Option<String>,
    ) -> anyhow::Result<Value>;
    fn show(&self, bug_id: &str) -> anyhow::Result<Value>;
    fn ready(&self, project: Option<String>) -> anyhow::Result<Value>;
    #[allow(clippy::too_many_arguments)]
    fn update(
        &self,
        bug_id: &str,
        status: Option<String>,
        severity: Option<String>,
        priority: Option<String>,
        component: Option<String>,
        owner: Option<String>,
    ) -> anyhow::Result<Value>;
    fn record_attempt(&self, bug_id: &str, attempt: Value) -> anyhow::Result<Value>;
}
