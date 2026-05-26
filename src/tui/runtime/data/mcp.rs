//! Owner: TUI runtime - McpResourceDataClient (stub).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::data::mcp`
//! Invariants: This file is intentionally a stub. MCP resource bindings
//!             arrive in a U10 follow-up once the read plane HTTP
//!             contracts are stable (per plan §10 + §12). Each method
//!             returns `Err` (no panics) so the UI can render a degraded
//!             badge if a caller picks this transport prematurely.

use anyhow::Result;
use async_trait::async_trait;

use super::client::{DataClient, EventPage};
use crate::api::actions::{ActionPreview, ActionResult};
use crate::api::proof::{ProofQuery, ProofTimeline};
use crate::api::read_model::TuiReadModel;
use crate::api::runtime_profile::RuntimeProfile;

const NOT_WIRED: &str = "MCP resource client not yet wired (U10 follow-up)";

/// Stub `DataClient` for MCP resources. Every method returns `Err`.
#[derive(Debug, Clone, Default)]
pub struct McpResourceDataClient;

#[async_trait]
impl DataClient for McpResourceDataClient {
    async fn fetch_read_model(&self) -> Result<TuiReadModel> {
        anyhow::bail!(NOT_WIRED)
    }
    async fn fetch_events(&self, _cursor: u64, _limit: u32) -> Result<EventPage> {
        anyhow::bail!(NOT_WIRED)
    }
    async fn fetch_proof(&self, _q: ProofQuery) -> Result<ProofTimeline> {
        anyhow::bail!(NOT_WIRED)
    }
    async fn fetch_entity(&self, _k: &str, _id: &str) -> Result<serde_json::Value> {
        anyhow::bail!(NOT_WIRED)
    }
    async fn fetch_runtime_profile(&self) -> Result<RuntimeProfile> {
        anyhow::bail!(NOT_WIRED)
    }
    async fn fetch_action_registry(&self) -> Result<Vec<serde_json::Value>> {
        anyhow::bail!(NOT_WIRED)
    }
    async fn preview_action(
        &self,
        _action_id: &str,
        _args: serde_json::Value,
    ) -> Result<ActionPreview> {
        anyhow::bail!(NOT_WIRED)
    }
    async fn execute_action(
        &self,
        _action_id: &str,
        _args: serde_json::Value,
        _idempotency_key: Option<String>,
    ) -> Result<ActionResult> {
        anyhow::bail!(NOT_WIRED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mcp_client_is_a_stub() {
        let c = McpResourceDataClient;
        assert!(c.fetch_read_model().await.is_err());
        assert!(c.fetch_events(0, 10).await.is_err());
        assert!(c.fetch_runtime_profile().await.is_err());
    }
}
