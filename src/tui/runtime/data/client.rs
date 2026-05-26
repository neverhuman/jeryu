//! Owner: TUI runtime - DataClient trait. The single async surface every
//! TUI screen calls into; concrete transports (HTTP/Fixture/MCP/Local)
//! implement it. The UI never sees a `reqwest::Client` directly.
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::data::client`
//! Invariants:
//!   - All methods are async + `Send` (so the TUI runtime can spawn them
//!     on a tokio task).
//!   - Errors return `anyhow::Result`. Concrete implementations are
//!     expected to attach `context` describing the transport (e.g.
//!     `"http: GET /api/v1/read-model"`).
//!   - `fetch_action_registry` returns the raw `serde_json::Value` array
//!     so the UI can keep rendering even if `ActionEntry::contract_json()`
//!     evolves (forward-compatibility per plan §17 U06/U08 note).
//!   - `idempotency_key` on `execute_action` is `Option<String>` so the
//!     UI can choose: human-triggered actions pass `None`; agent-driven
//!     actions pass a key derived from the run.

use anyhow::Result;
use async_trait::async_trait;

use crate::api::actions::{ActionPreview, ActionResult};
use crate::api::proof::{ProofQuery, ProofTimeline};
use crate::api::read_model::TuiReadModel;
use crate::api::runtime_profile::RuntimeProfile;

// Re-export so callers say `use crate::tui::runtime::data::EventPage;`
// instead of reaching across modules. EventPage is the canonical paged
// event response shape defined by the inspection plane (U07).
pub use crate::inspection::events::EventPage;

/// The async data surface the TUI consumes. Implementations must be
/// `Send + Sync` so they can be wrapped in `Arc<dyn DataClient>` and
/// shared across the runtime + render threads.
#[async_trait]
pub trait DataClient: Send + Sync {
    /// Fetch the typed read model the TUI renders from.
    async fn fetch_read_model(&self) -> Result<TuiReadModel>;

    /// Fetch a page of events starting at `cursor`. Bounded by `limit`.
    /// Concrete transports should clamp `limit` to a sane max (the
    /// inspection plane caps at 500).
    async fn fetch_events(&self, cursor: u64, limit: u32) -> Result<EventPage>;

    /// Fetch the proof timeline matching `query`. Cursor is opaque.
    async fn fetch_proof(&self, query: ProofQuery) -> Result<ProofTimeline>;

    /// Fetch an entity by (kind, id). Returns the raw projection so the
    /// caller can switch on `kind` and decode into the right typed
    /// variant (`EntityKind` map lives in `crate::api::entity`).
    async fn fetch_entity(&self, kind: &str, id: &str) -> Result<serde_json::Value>;

    /// Fetch the runtime profile (backends, schema hash, redacted URLs).
    async fn fetch_runtime_profile(&self) -> Result<RuntimeProfile>;

    /// Fetch the action registry as raw JSON entries. The UI keeps the
    /// JSON shape to stay forward-compatible with U06 RiskTier evolution.
    async fn fetch_action_registry(&self) -> Result<Vec<serde_json::Value>>;

    /// Compute what an action would do, without executing it. The
    /// returned `ActionPreview` powers the confirmation modal.
    async fn preview_action(
        &self,
        action_id: &str,
        args: serde_json::Value,
    ) -> Result<ActionPreview>;

    /// Execute an action. `idempotency_key` deduplicates retries
    /// (servers SHOULD reject duplicate keys with the cached result).
    async fn execute_action(
        &self,
        action_id: &str,
        args: serde_json::Value,
        idempotency_key: Option<String>,
    ) -> Result<ActionResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // A handwritten stub impl to assert the trait shape compiles and is
    // object-safe. Each method returns an error so this never has to
    // synthesize realistic data.
    struct UnimplementedClient;

    #[async_trait]
    impl DataClient for UnimplementedClient {
        async fn fetch_read_model(&self) -> Result<TuiReadModel> {
            anyhow::bail!("stub")
        }
        async fn fetch_events(&self, _cursor: u64, _limit: u32) -> Result<EventPage> {
            anyhow::bail!("stub")
        }
        async fn fetch_proof(&self, _query: ProofQuery) -> Result<ProofTimeline> {
            anyhow::bail!("stub")
        }
        async fn fetch_entity(&self, _kind: &str, _id: &str) -> Result<serde_json::Value> {
            anyhow::bail!("stub")
        }
        async fn fetch_runtime_profile(&self) -> Result<RuntimeProfile> {
            anyhow::bail!("stub")
        }
        async fn fetch_action_registry(&self) -> Result<Vec<serde_json::Value>> {
            anyhow::bail!("stub")
        }
        async fn preview_action(
            &self,
            _action_id: &str,
            _args: serde_json::Value,
        ) -> Result<ActionPreview> {
            anyhow::bail!("stub")
        }
        async fn execute_action(
            &self,
            _action_id: &str,
            _args: serde_json::Value,
            _idempotency_key: Option<String>,
        ) -> Result<ActionResult> {
            anyhow::bail!("stub")
        }
    }

    #[test]
    fn data_client_trait_is_object_safe() {
        // Compile-time check: `dyn DataClient` must be a valid type.
        // If the trait grows a non-object-safe method later, this fails.
        let _boxed: Box<dyn DataClient> = Box::new(UnimplementedClient);
    }

    #[tokio::test]
    async fn unimplemented_stub_returns_error_not_panic() {
        let client = UnimplementedClient;
        assert!(client.fetch_read_model().await.is_err());
    }
}
