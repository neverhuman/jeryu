//! Owner: Inspection HTTP plane - axum Router builder for /api/v1/* routes.
//! Proof: `cargo test -p jeryu --lib inspection::router`
//! Invariants: All read routes mounted here; mutating routes live in
//!             `inspection::actions` (U08) and are NOT added by this builder.

use axum::Router;
use axum::routing::get;

use super::entity::get_entity;
use super::events::{get_events, get_events_stream};
use super::health::{get_health_deep, get_runtime_profile};
use super::proof::get_proof;
use super::read_model::get_read_model;
use super::state::InspectionState;

/// Build the read-only `/api/v1/*` router. U08 will extend this with
/// `/api/v1/action/*` endpoints in a follow-up unit.
pub fn build_router(state: InspectionState) -> Router {
    Router::new()
        .route("/api/v1/read-model", get(get_read_model))
        .route("/api/v1/events", get(get_events))
        .route("/api/v1/events/stream", get(get_events_stream))
        .route("/api/v1/entity/{kind}/{id}", get(get_entity))
        .route("/api/v1/proof", get(get_proof))
        .route("/api/v1/runtime/profile", get(get_runtime_profile))
        .route("/api/v1/health/deep", get(get_health_deep))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_router_constructs_without_panic() {
        let _ = build_router(InspectionState::default());
    }
}
