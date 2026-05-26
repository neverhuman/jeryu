//! Owner: TUI runtime - TraceDataClient (log + job-trace fetcher).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::data::trace`
//! Invariants:
//!   - This is the surface the Logs lens (plan §16 / §17 U20) will use
//!     for log+trace fetch. The transport is currently a stub returning
//!     an explanatory error so the UI can render `NO PROOF` rather than
//!     panic when a screen requests a trace prematurely.
//!   - The trait is intentionally narrow: one `fetch_trace(kind, id)`
//!     method. Streaming trace updates (subscription) lands when the
//!     events store grows job-log chunk events upstream.

use anyhow::Result;
use async_trait::async_trait;

use crate::api::entity::EntityRef;

const NOT_WIRED: &str = "trace not yet wired (U10 follow-up)";

/// Async fetcher for textual log/job-trace payloads. Separate from
/// `DataClient` because traces flow over a different storage path
/// (object store / log scraper) than the typed read model.
#[async_trait]
pub trait TraceClient: Send + Sync {
    /// Fetch a textual trace for `(entity_kind, id)`. Implementations
    /// SHOULD truncate to a sane size and indicate truncation in the
    /// returned string when applicable.
    async fn fetch_trace(&self, entity_kind: &str, id: &str) -> Result<String>;

    /// Convenience wrapper for typed callers.
    async fn fetch_trace_for(&self, entity: &EntityRef) -> Result<String> {
        let kind_label: String = serde_json::to_value(entity.kind)?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        self.fetch_trace(&kind_label, &entity.id).await
    }
}

/// Stub `TraceClient` for the U10 cut. Returns an explanatory error so
/// the Logs lens can render a "TRACE STUB" badge instead of crashing.
#[derive(Debug, Clone, Default)]
pub struct TraceDataClient;

#[async_trait]
impl TraceClient for TraceDataClient {
    async fn fetch_trace(&self, _entity_kind: &str, _id: &str) -> Result<String> {
        anyhow::bail!(NOT_WIRED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::EntityKind;

    #[tokio::test]
    async fn trace_client_is_a_stub() {
        let c = TraceDataClient;
        let err = c.fetch_trace("job", "abc").await.unwrap_err();
        assert!(
            err.to_string().contains("trace not yet wired"),
            "expected stub error, got: {err}"
        );
    }

    #[tokio::test]
    async fn trace_client_for_entity_resolves_kind_label() {
        let c = TraceDataClient;
        let entity = EntityRef::new(EntityKind::Job, "j-1");
        assert!(c.fetch_trace_for(&entity).await.is_err());
    }
}
