//! Owner: MCP adapter for external coding agents (standalone, self-contained crate).
//! Proof: `cargo build && cargo test` inside this crate's directory.
//!
//! Invariants:
//! - MCP request handling stays a thin adapter over a `ToolBackend` trait seam;
//!   it must not bypass grant/evidence/merge gates that the real backend enforces.
//! - HTTP transport is loopback-only and validates MCP session headers.
//! - stdio and HTTP route every tool call through the same `ToolBackend::call` path.
//!
//! The `ToolBackend`/`BugStore` traits abstract the engine. `BridgeBackend` is the
//! real implementation: it routes the mutating tools through `jeryu-agentbridge`
//! for scope-validated bounded mutations. A deterministic in-memory `MemoryBackend`
//! is retained as a test double.

pub mod backend;
pub mod core;
pub mod http;
pub mod tools;

pub use backend::{BridgeBackend, BugStore, MemoryBackend, ToolBackend, ToolResponse};
pub use core::{McpCore, McpSessionState, start_mcp_stdio};
pub use http::{McpHttpState, mcp_router, start_mcp_http};
pub use tools::{ToolDescriptor, tool_manifest};

/// MCP protocol version this server speaks. This is the MCP spec version, not a brand literal.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// Namespace prefix for every core tool name (`jeryu.fetch_capsule`, ...).
pub const TOOL_PREFIX: &str = "jeryu.";
