//! Owner: TUI runtime - LocalDataClient (stub).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::data::local`
//! Invariants: Degraded local fallback (DB-owned adapters only) is not
//!             wired here yet. Plan §12 designates this transport for
//!             "approved DB-owned adapters" — that wiring lands when
//!             the DB boundary discipline (§4) has owners assigned.

use anyhow::Result;
use async_trait::async_trait;

use super::client::{DataClient, EventPage};
use crate::api::actions::{ActionPreview, ActionResult};
use crate::api::proof::{ProofQuery, ProofTimeline};
use crate::api::read_model::TuiReadModel;
use crate::api::runtime_profile::RuntimeProfile;

const NOT_WIRED: &str = "Local data client not yet wired (U10 follow-up)";

/// Stub `DataClient` for local DB fallback. Every method returns `Err`.
#[derive(Debug, Clone, Default)]
pub struct LocalDataClient;

#[async_trait]
impl DataClient for LocalDataClient {
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
    async fn local_client_is_a_stub() {
        let c = LocalDataClient;
        assert!(c.fetch_read_model().await.is_err());
        assert!(c.fetch_entity("job", "x").await.is_err());
    }
}
