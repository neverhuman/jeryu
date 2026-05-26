//! Owner: TUI Control-Plane API - inspection read contracts
//! Proof: `cargo test -p jeryu --lib api::inspection`
//! Invariants: inspection responses are typed, versioned, and read-only.
//!             Every `/api/v1/*` read route returns `InspectionEnvelope<T>`
//!             so callers receive `api_version`, `generated_at`, and
//!             `sources` (freshness sidecar) alongside the payload.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::freshness::SourceFreshness;

/// API version pinned in every inspection response envelope. Callers MAY
/// reject responses whose `api_version` differs (forward-compatibility
/// is opt-in via env, not implicit).
pub const INSPECTION_API_VERSION: &str = "api.v1";

/// Wire envelope every `/api/v1/*` read handler returns. Mutating
/// endpoints (`/api/v1/action/preview`, `/api/v1/action/execute`,
/// `/api/v1/action/{run_id}/stream`) are NOT envelope-wrapped — they
/// have their own typed responses (`ActionPreview`, `ExecuteResponse`,
/// SSE).
///
/// `T` is the typed payload (e.g. `TuiReadModel`, `EventPage`,
/// `EntityDetail`, `ProofTimeline`, `RuntimeProfile`, `DeepHealth`,
/// `ActionRegistryDocument`). `sources` is the daemon's view of which
/// upstreams it consulted; first-cut is empty until the projection
/// loop wires real freshness records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InspectionEnvelope<T> {
    pub api_version: String,
    pub generated_at: DateTime<Utc>,
    pub data: T,
    pub sources: Vec<SourceFreshness>,
}

impl<T> InspectionEnvelope<T> {
    /// Construct an envelope with the canonical api version and an
    /// explicit `generated_at` (tests freeze the clock).
    pub fn new(data: T, sources: Vec<SourceFreshness>, generated_at: DateTime<Utc>) -> Self {
        Self {
            api_version: INSPECTION_API_VERSION.into(),
            generated_at,
            data,
            sources,
        }
    }

    /// Convenience: empty sources, `generated_at = Utc::now()`. Used by
    /// handlers that do not yet have freshness wiring.
    pub fn now(data: T) -> Self {
        Self::new(data, Vec::new(), Utc::now())
    }

    /// Consume the envelope and return the payload. Downstream callers
    /// (e.g. `HttpDataClient`) unwrap so the TUI never sees the wire
    /// shape.
    pub fn into_data(self) -> T {
        self.data
    }
}

/// Action registry document. Wraps the JSON entry list with a count and
/// an `api_version` mirror so palettes can short-circuit when the
/// versions disagree without re-decoding every entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionRegistryDocument {
    pub api_version: String,
    pub action_count: usize,
    pub actions: Vec<serde_json::Value>,
}

impl ActionRegistryDocument {
    pub fn new(actions: Vec<serde_json::Value>) -> Self {
        Self {
            api_version: INSPECTION_API_VERSION.into(),
            action_count: actions.len(),
            actions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::freshness::{SourceFreshness, SourceKind};

    #[test]
    fn envelope_pins_api_version() {
        let now = Utc::now();
        let envelope = InspectionEnvelope::new(
            42_u32,
            vec![SourceFreshness::live(SourceKind::InspectionHttp, now, "9")],
            now,
        );

        assert_eq!(envelope.api_version, INSPECTION_API_VERSION);
        assert_eq!(envelope.data, 42);
        assert_eq!(envelope.sources.len(), 1);
        assert_eq!(envelope.generated_at, now);
    }

    #[test]
    fn envelope_now_has_empty_sources() {
        let envelope = InspectionEnvelope::now("hello");
        assert_eq!(envelope.api_version, INSPECTION_API_VERSION);
        assert!(envelope.sources.is_empty());
        assert_eq!(envelope.data, "hello");
    }

    #[test]
    fn envelope_into_data_returns_payload() {
        let envelope = InspectionEnvelope::now(vec![1_u8, 2, 3]);
        let data = envelope.into_data();
        assert_eq!(data, vec![1, 2, 3]);
    }

    #[test]
    fn action_registry_document_counts_actions() {
        let doc = ActionRegistryDocument::new(vec![
            serde_json::json!({"id": "open_logs"}),
            serde_json::json!({"id": "requeue_job"}),
        ]);
        assert_eq!(doc.action_count, 2);
        assert_eq!(doc.api_version, INSPECTION_API_VERSION);
        assert_eq!(doc.actions.len(), 2);
    }

    #[test]
    fn action_registry_document_empty_zero_count() {
        let doc = ActionRegistryDocument::new(Vec::new());
        assert_eq!(doc.action_count, 0);
        assert!(doc.actions.is_empty());
    }

    #[test]
    fn envelope_serde_round_trips() {
        let now = Utc::now();
        let envelope = InspectionEnvelope::new("payload".to_string(), Vec::new(), now);
        let json = serde_json::to_string(&envelope).unwrap();
        let back: InspectionEnvelope<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope, back);
    }
}
