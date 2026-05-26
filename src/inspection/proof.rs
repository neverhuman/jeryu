//! Owner: Inspection HTTP plane - GET /api/v1/proof?... handler.
//! Proof: `cargo test -p jeryu --lib inspection::proof`
//! Invariants: Query parameters never reach DB layer as raw SQL; limits are
//!             always capped via `ProofQuery::effective_limit`. Responses
//!             are `InspectionEnvelope<ProofTimeline>` so the wire shape
//!             matches every other inspection read route.

use axum::Json;
use axum::extract::{Query, State};
use chrono::Utc;

use crate::api::inspection::InspectionEnvelope;
use crate::api::proof::{ProofQuery, ProofTimeline};

use super::state::InspectionState;

pub async fn get_proof(
    Query(q): Query<ProofQuery>,
    State(state): State<InspectionState>,
) -> Json<InspectionEnvelope<ProofTimeline>> {
    let _limit = q.effective_limit();
    let timeline = ProofTimeline {
        entries: Vec::new(),
        next_cursor: None,
    };
    let sources = state.snapshot_sources();
    Json(InspectionEnvelope::new(timeline, sources, Utc::now()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::inspection::INSPECTION_API_VERSION;

    #[tokio::test]
    async fn proof_handler_returns_empty_timeline_in_envelope() {
        let state = InspectionState::default();
        let Json(envelope) = get_proof(Query(ProofQuery::default()), State(state)).await;
        assert_eq!(envelope.api_version, INSPECTION_API_VERSION);
        assert!(envelope.data.entries.is_empty());
        assert!(envelope.data.next_cursor.is_none());
    }
}
