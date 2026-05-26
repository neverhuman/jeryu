//! Owner: Inspection HTTP plane - GET /api/v1/read-model handler.
//! Proof: `cargo test -p jeryu --lib inspection::read_model`
//! Invariants: Returns the typed TuiReadModel snapshot the TUI consumes
//!             wrapped in `InspectionEnvelope<TuiReadModel>`; handler
//!             never reads from raw DB/Docker/GitLab directly.

use axum::Json;
use axum::extract::State;
use chrono::Utc;

use crate::api::inspection::InspectionEnvelope;
use crate::api::read_model::TuiReadModel;

use super::state::InspectionState;

pub async fn get_read_model(
    State(state): State<InspectionState>,
) -> Json<InspectionEnvelope<TuiReadModel>> {
    let sources = state.snapshot_sources();
    Json(InspectionEnvelope::new(state.read_model(), sources, Utc::now()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::inspection::INSPECTION_API_VERSION;

    #[tokio::test]
    async fn read_model_handler_returns_state_snapshot_in_envelope() {
        let state = InspectionState::default();
        let Json(envelope) = get_read_model(State(state.clone())).await;
        assert_eq!(envelope.api_version, INSPECTION_API_VERSION);
        assert_eq!(
            envelope.data.schema_version,
            state.read_model().schema_version
        );
    }

    #[tokio::test]
    async fn read_model_envelope_starts_with_empty_sources() {
        let state = InspectionState::default();
        let Json(envelope) = get_read_model(State(state)).await;
        assert!(envelope.sources.is_empty());
    }
}
