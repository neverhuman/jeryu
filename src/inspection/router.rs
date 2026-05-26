//! Owner: Inspection HTTP plane - axum Router builder for /api/v1/* routes.
//! Proof: `cargo test -p jeryu --lib inspection::router`
//! Invariants: All read routes mounted here; mutating routes live in
//!             `inspection::actions` (U08) as stubs and are wired below
//!             alongside the read plane.

use axum::Router;
use axum::routing::{get, post};

use super::actions::{
    get_action_registry, get_action_stream, post_action_execute, post_action_preview,
};
use super::entity::get_entity;
use super::events::{get_events, get_events_stream};
use super::health::{get_health_deep, get_runtime_profile};
use super::proof::get_proof;
use super::read_model::get_read_model;
use super::state::InspectionState;

/// Build the `/api/v1/*` router (read plane + U08 action endpoints).
pub fn build_router(state: InspectionState) -> Router {
    Router::new()
        .route("/api/v1/read-model", get(get_read_model))
        .route("/api/v1/events", get(get_events))
        .route("/api/v1/events/stream", get(get_events_stream))
        .route("/api/v1/entity/{kind}/{id}", get(get_entity))
        .route("/api/v1/proof", get(get_proof))
        .route("/api/v1/runtime/profile", get(get_runtime_profile))
        .route("/api/v1/health/deep", get(get_health_deep))
        .route("/api/v1/action/preview", post(post_action_preview))
        .route("/api/v1/action/execute", post(post_action_execute))
        .route("/api/v1/action/{run_id}/stream", get(get_action_stream))
        .route("/api/v1/action-registry", get(get_action_registry))
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
