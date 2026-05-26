//! Owner: Inspection HTTP plane - GET /api/v1/proof?... handler.
//! Proof: `cargo test -p jeryu --lib inspection::proof`
//! Invariants: Query parameters never reach DB layer as raw SQL; limits are
//!             always capped via `ProofQuery::effective_limit`.

use axum::Json;
use axum::extract::{Query, State};

use crate::api::proof::{ProofQuery, ProofTimeline};

use super::state::InspectionState;

pub async fn get_proof(
    Query(q): Query<ProofQuery>,
    State(_state): State<InspectionState>,
) -> Json<ProofTimeline> {
    let _limit = q.effective_limit();
    Json(ProofTimeline {
        entries: Vec::new(),
        next_cursor: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn proof_handler_returns_empty_timeline() {
        let state = InspectionState::default();
        let Json(timeline) = get_proof(Query(ProofQuery::default()), State(state)).await;
        assert!(timeline.entries.is_empty());
        assert!(timeline.next_cursor.is_none());
    }
}
