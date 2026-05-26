//! Owner: Inspection HTTP plane - GET /api/v1/events + /api/v1/events/stream.
//! Proof: `cargo test -p jeryu --lib inspection::events`
//! Invariants: Cursor pagination is monotonic; stream endpoint returns an
//!             SSE-formatted response. Live event sourcing wiring lands in
//!             U10 (TUI DataClient); this handler returns the empty page
//!             until the daemon wires its event projection. The page is
//!             wrapped in `InspectionEnvelope<EventPage>` on the wire so
//!             api version + sources sidecar travel with every response.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::api::inspection::InspectionEnvelope;

use super::state::InspectionState;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EventQuery {
    pub cursor: Option<u64>,
    pub limit: Option<u32>,
    pub kind: Option<String>,
    pub entity_kind: Option<String>,
    pub entity_id: Option<String>,
}

impl EventQuery {
    pub fn effective_limit(&self) -> u32 {
        self.limit.map(|n| n.min(500)).unwrap_or(100)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventPage {
    pub cursor: u64,
    pub next_cursor: Option<u64>,
    pub items: Vec<serde_json::Value>,
}

pub async fn get_events(
    Query(q): Query<EventQuery>,
    State(state): State<InspectionState>,
) -> Json<InspectionEnvelope<EventPage>> {
    let cursor = q.cursor.unwrap_or_else(|| state.read_model().event_cursor);
    let _limit = q.effective_limit();
    let page = EventPage {
        cursor,
        next_cursor: None,
        items: Vec::new(),
    };
    let sources = state.snapshot_sources();
    Json(InspectionEnvelope::new(page, sources, Utc::now()))
}

pub async fn get_events_stream(Query(_q): Query<EventQuery>) -> Response {
    let body = "event: heartbeat\ndata: {\"alive\":true}\n\n";
    (
        StatusCode::OK,
        [
            ("content-type", "text/event-stream"),
            ("cache-control", "no-cache"),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::inspection::INSPECTION_API_VERSION;

    #[tokio::test]
    async fn get_events_returns_empty_page_with_current_cursor() {
        let state = InspectionState::default();
        let Json(envelope) =
            get_events(Query(EventQuery::default()), State(state.clone())).await;
        assert_eq!(envelope.api_version, INSPECTION_API_VERSION);
        assert_eq!(envelope.data.cursor, state.read_model().event_cursor);
        assert!(envelope.data.items.is_empty());
    }

    #[test]
    fn event_query_limit_is_capped() {
        let q = EventQuery {
            limit: Some(100_000),
            ..EventQuery::default()
        };
        assert_eq!(q.effective_limit(), 500);
    }

    #[tokio::test]
    async fn event_stream_returns_text_event_stream_content_type() {
        let resp = get_events_stream(Query(EventQuery::default())).await;
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        assert_eq!(ct, "text/event-stream");
    }
}
