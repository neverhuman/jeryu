//! Owner: TUI runtime - FixtureDataClient (deterministic in-memory client).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::data::fixture`
//! Invariants:
//!   - All returned data is hard-coded; no I/O. This is the client the
//!     screenshot harness, demo flag, and unit tests use so render
//!     output stays reproducible.
//!   - `event_cursor` advances on each `fetch_events` call so PollStream
//!     tests can drive monotonic-cursor behaviour without a real server.
//!   - `preview_action` always returns a disabled preview shape and
//!     `execute_action` always returns `Rejected` so fixture clients
//!     cannot accidentally trigger mutations during demo flows.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use async_trait::async_trait;

use super::client::{DataClient, EventPage};
use crate::api::actions::{ActionPreview, ActionResult, ActionStatus};
use crate::api::proof::{ProofQuery, ProofTimeline};
use crate::api::read_model::TuiReadModel;
use crate::api::runtime_profile::RuntimeProfile;
use crate::tui::action_registry::{GrantRequirement, RiskTier, SideEffectClass};

/// Deterministic, in-memory `DataClient` for demos and tests. Cloning is
/// cheap and shares the same `event_cursor` so multiple consumers see
/// consistent monotonic ordering.
#[derive(Debug, Clone)]
pub struct FixtureDataClient {
    read_model: TuiReadModel,
    runtime_profile: RuntimeProfile,
    cursor: Arc<AtomicU64>,
}

impl Default for FixtureDataClient {
    fn default() -> Self {
        Self::new(
            TuiReadModel::default(),
            RuntimeProfile::new("fixture", "memory", "memory"),
        )
    }
}

impl FixtureDataClient {
    pub fn new(read_model: TuiReadModel, runtime_profile: RuntimeProfile) -> Self {
        Self {
            read_model,
            runtime_profile,
            cursor: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Currently-stored event cursor (for assertions in tests).
    pub fn cursor(&self) -> u64 {
        self.cursor.load(Ordering::SeqCst)
    }

    /// Manually advance the cursor (so a test can simulate a backlog).
    pub fn set_cursor(&self, next: u64) {
        self.cursor.store(next, Ordering::SeqCst);
    }
}

#[async_trait]
impl DataClient for FixtureDataClient {
    async fn fetch_read_model(&self) -> Result<TuiReadModel> {
        let mut snapshot = self.read_model.clone();
        // Reflect the in-memory cursor so the model and event stream
        // stay aligned even if the caller is polling both.
        snapshot.event_cursor = self.cursor.load(Ordering::SeqCst);
        Ok(snapshot)
    }

    async fn fetch_events(&self, cursor: u64, _limit: u32) -> Result<EventPage> {
        // Fixture never invents events; it only confirms the cursor
        // advances when callers ask. This is the shape PollStream
        // tests use to assert monotonic ordering without a real
        // server.
        let current = self.cursor.load(Ordering::SeqCst);
        let next = current.max(cursor);
        self.cursor.store(next, Ordering::SeqCst);
        Ok(EventPage {
            cursor: next,
            next_cursor: None,
            items: Vec::new(),
        })
    }

    async fn fetch_proof(&self, _query: ProofQuery) -> Result<ProofTimeline> {
        Ok(ProofTimeline {
            entries: Vec::new(),
            next_cursor: None,
        })
    }

    async fn fetch_entity(&self, kind: &str, id: &str) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "kind": kind,
            "id": id,
            "fixture": true,
        }))
    }

    async fn fetch_runtime_profile(&self) -> Result<RuntimeProfile> {
        Ok(self.runtime_profile.clone())
    }

    async fn fetch_action_registry(&self) -> Result<Vec<serde_json::Value>> {
        // Empty array is acceptable for fixtures; the real registry is
        // wired via HttpDataClient. Tests that assert on registry shape
        // should hit the HTTP path against a spawned server.
        Ok(Vec::new())
    }

    async fn preview_action(
        &self,
        action_id: &str,
        _args: serde_json::Value,
    ) -> Result<ActionPreview> {
        Ok(ActionPreview {
            enabled: false,
            disabled_reason: Some("fixture client cannot preview real actions".into()),
            risk: RiskTier::R0,
            side_effect_class: SideEffectClass::ReadOnly,
            side_effects: Vec::new(),
            will_not: Vec::new(),
            summary: format!("fixture preview for {action_id}"),
            evidence_expected: Vec::new(),
            required_grant: GrantRequirement::None,
            undo_action: None,
            confirm_prompt: None,
        })
    }

    async fn execute_action(
        &self,
        action_id: &str,
        _args: serde_json::Value,
        _idempotency_key: Option<String>,
    ) -> Result<ActionResult> {
        Ok(ActionResult {
            status: ActionStatus::Rejected,
            summary: format!("fixture client rejected execute of {action_id}"),
            event_cursor: Some(self.cursor.load(Ordering::SeqCst)),
            affected_entity: None,
            evidence_created: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fixture_client_returns_default() {
        let client = FixtureDataClient::default();
        assert!(client.fetch_read_model().await.is_ok());
    }

    #[tokio::test]
    async fn fixture_read_model_carries_schema_version() {
        let client = FixtureDataClient::default();
        let model = client.fetch_read_model().await.unwrap();
        assert_eq!(model.schema_version, crate::api::read_model::SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn fixture_cursor_is_monotonic() {
        let client = FixtureDataClient::default();
        let p1 = client.fetch_events(0, 10).await.unwrap();
        let p2 = client.fetch_events(5, 10).await.unwrap();
        assert!(p2.cursor >= p1.cursor);
        assert_eq!(p2.cursor, 5);
    }

    #[tokio::test]
    async fn fixture_execute_is_rejected() {
        let client = FixtureDataClient::default();
        let r = client
            .execute_action("open_logs", serde_json::json!({}), None)
            .await
            .unwrap();
        assert_eq!(r.status, ActionStatus::Rejected);
    }
}
