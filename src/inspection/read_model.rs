//! Owner: Inspection HTTP plane - GET /api/v1/read-model handler.
//! Proof: `cargo test -p jeryu --lib inspection::read_model`
//! Invariants: Returns the typed TuiReadModel snapshot the TUI consumes;
//!             handler never reads from raw DB/Docker/GitLab directly.

use axum::Json;
use axum::extract::State;

use crate::api::read_model::TuiReadModel;

use super::state::InspectionState;

pub async fn get_read_model(State(state): State<InspectionState>) -> Json<TuiReadModel> {
    Json(state.read_model())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_model_handler_returns_state_snapshot() {
        let state = InspectionState::default();
        let Json(model) = get_read_model(State(state.clone())).await;
        assert_eq!(model.schema_version, state.read_model().schema_version);
    }
}
