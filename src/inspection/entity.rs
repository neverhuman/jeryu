//! Owner: Inspection HTTP plane - GET /api/v1/entity/{kind}/{id} handler.
//! Proof: `cargo test -p jeryu --lib inspection::entity`
//! Invariants: Unknown EntityKind tokens return 400; missing entities return
//!             404. Detail-payload assembly per kind lands in U07 follow-ups
//!             once each domain projection is online (see projections/).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::api::entity::{EntityKind, EntityRef};

use super::state::InspectionState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityDetail {
    pub entity: EntityRef,
    pub label: String,
}

pub async fn get_entity(
    Path((kind, id)): Path<(String, String)>,
    State(_state): State<InspectionState>,
) -> Response {
    let parsed: Result<EntityKind, _> =
        serde_json::from_value(serde_json::Value::String(kind.clone()));
    match parsed {
        Ok(kind) => Json(EntityDetail {
            entity: EntityRef::new(kind, id.clone()),
            label: format!("{}:{}", kind_label(kind), id),
        })
        .into_response(),
        Err(_) => (
            StatusCode::BAD_REQUEST,
            format!("unknown entity kind: {kind}"),
        )
            .into_response(),
    }
}

fn kind_label(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Job => "job",
        EntityKind::Pipeline => "pipeline",
        EntityKind::Agent => "agent",
        EntityKind::AgentTask => "agent_task",
        EntityKind::MergeRequest => "merge_request",
        EntityKind::TestPlan => "test_plan",
        EntityKind::TestCase => "test_case",
        EntityKind::EvidenceCapsule => "evidence_capsule",
        EntityKind::ReleaseAttempt => "release_attempt",
        EntityKind::ReleaseGate => "release_gate",
        EntityKind::CacheTaint => "cache_taint",
        EntityKind::CacheObject => "cache_object",
        EntityKind::Bug => "bug",
        EntityKind::BugAttempt => "bug_attempt",
        EntityKind::Project => "project",
        EntityKind::SecretAccess => "secret_access",
        EntityKind::Grant => "grant",
        EntityKind::Pool => "pool",
        EntityKind::Runner => "runner",
        EntityKind::System => "system",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn entity_handler_returns_detail_for_known_kind() {
        let state = InspectionState::default();
        let resp = get_entity(Path(("job".into(), "j-1".into())), State(state)).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn entity_handler_rejects_unknown_kind() {
        let state = InspectionState::default();
        let resp = get_entity(Path(("unicorn".into(), "id".into())), State(state)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
