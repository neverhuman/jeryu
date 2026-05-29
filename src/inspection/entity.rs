//! Owner: Inspection HTTP plane - GET /api/v1/entity/{kind}/{id} handler.
//! Proof: `cargo test -p jeryu --lib inspection::entity`
//! Invariants: Unknown EntityKind tokens return 400; missing entities return
//!             404. Detail-payload assembly per kind lands in U07 follow-ups
//!             once each domain projection is online (see projections/).
//!             Successful responses are `InspectionEnvelope<EntityDetail>`
//!             so the envelope contract holds even on per-entity reads.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::api::entity::{EntityKind, EntityRef};
use crate::api::inspection::InspectionEnvelope;

use super::state::InspectionState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityDetail {
    pub entity: EntityRef,
    pub label: String,
}

pub async fn get_entity(
    Path((kind, id)): Path<(String, String)>,
    State(state): State<InspectionState>,
) -> Response {
    let parsed: Result<EntityKind, _> =
        serde_json::from_value(serde_json::Value::String(kind.clone()));
    match parsed {
        Ok(kind) => {
            let detail = EntityDetail {
                entity: EntityRef::new(kind, id.clone()),
                label: format!("{}:{}", kind_label(kind), id),
            };
            let envelope = InspectionEnvelope::new(detail, state.snapshot_sources(), Utc::now());
            Json(envelope).into_response()
        }
        Err(_) => (
            StatusCode::BAD_REQUEST,
            format!("unknown entity kind: {kind}"),
        )
            .into_response(),
    }
}

fn kind_label(kind: EntityKind) -> &'static str {
    kind.label()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::inspection::INSPECTION_API_VERSION;

    #[tokio::test]
    async fn entity_handler_returns_detail_for_known_kind() {
        let state = InspectionState::default();
        let resp = get_entity(Path(("job".into(), "j-1".into())), State(state)).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn entity_handler_envelope_pins_api_version() {
        use axum::body::to_bytes;
        let state = InspectionState::default();
        let resp = get_entity(Path(("job".into(), "j-1".into())), State(state)).await;
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value.get("api_version").and_then(|v| v.as_str()),
            Some(INSPECTION_API_VERSION)
        );
        assert!(value.get("data").is_some());
    }

    #[tokio::test]
    async fn entity_handler_rejects_unknown_kind() {
        let state = InspectionState::default();
        let resp = get_entity(Path(("unicorn".into(), "id".into())), State(state)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
