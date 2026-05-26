//! Owner: Inspection HTTP plane - POST /api/v1/action/* + GET /api/v1/action-registry.
//! Proof: `cargo nextest run -p jeryu --lib inspection::actions`
//! Invariants: Mutating routes are stubs that return typed contract shapes
//!             only; no real execution happens here. Real action runtime
//!             wiring lands in U10 (TUI DataClient) + later units.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::api::actions::{ActionPreview, ActionResult, ActionStatus};
use crate::tui::action_registry::{GrantRequirement, REGISTRY, RiskTier, SideEffectClass};

use super::state::InspectionState;

// ── Request bodies ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct PreviewRequest {
    pub action_id: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteRequest {
    pub action_id: String,
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

// ── Response bodies ────────────────────────────────────────────────────

/// Response for POST /api/v1/action/execute. Carries the generated run_id
/// (callers stream events from /api/v1/action/{run_id}/stream) plus the
/// ActionResult for synchronous callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub run_id: String,
    pub status: ActionStatus,
    pub result: ActionResult,
}

// ── Handlers ───────────────────────────────────────────────────────────

/// POST /api/v1/action/preview - stub preview shape. Returns 200 with a
/// disabled ActionPreview until the action runtime wires real previews.
pub async fn post_action_preview(
    State(_state): State<InspectionState>,
    Json(req): Json<PreviewRequest>,
) -> Json<ActionPreview> {
    Json(ActionPreview {
        enabled: false,
        disabled_reason: Some("preview not yet wired".into()),
        risk: RiskTier::ReadOnly,
        side_effect_class: SideEffectClass::ReadOnly,
        side_effects: Vec::new(),
        will_not: Vec::new(),
        summary: format!("preview for {}", req.action_id),
        evidence_expected: Vec::new(),
        required_grant: GrantRequirement::None,
        undo_action: None,
        confirm_prompt: None,
    })
}

/// POST /api/v1/action/execute - stub execute. Returns 202 Accepted with a
/// newly generated run_id; does not actually execute.
pub async fn post_action_execute(
    State(_state): State<InspectionState>,
    Json(req): Json<ExecuteRequest>,
) -> Response {
    let run_id = uuid::Uuid::new_v4().to_string();
    let body = ExecuteResponse {
        run_id: run_id.clone(),
        status: ActionStatus::Accepted,
        result: ActionResult {
            status: ActionStatus::Accepted,
            summary: format!("accepted: {}", req.action_id),
            event_cursor: None,
            affected_entity: None,
            evidence_created: Vec::new(),
        },
    };
    (StatusCode::ACCEPTED, Json(body)).into_response()
}

/// GET /api/v1/action/{run_id}/stream - SSE heartbeat stub. Returns 200
/// with content-type text/event-stream and a single heartbeat event.
pub async fn get_action_stream(
    Path(run_id): Path<String>,
    State(_state): State<InspectionState>,
) -> Response {
    let body = format!("event: heartbeat\ndata: {{\"run_id\":\"{run_id}\",\"alive\":true}}\n\n");
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

/// GET /api/v1/action-registry - returns the static action registry as a
/// JSON array of contract entries. Safe to expose even while U06 RiskTier
/// migration is in flight because each entry serializes via the existing
/// `ActionEntry::contract_json()` helper.
pub async fn get_action_registry(
    State(_state): State<InspectionState>,
) -> Json<Vec<serde_json::Value>> {
    let entries: Vec<serde_json::Value> = REGISTRY.iter().map(|e| e.contract_json()).collect();
    Json(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspection::serve::serve_inspection;
    use reqwest::StatusCode as ReqwStatus;
    use tokio::net::TcpListener;

    async fn spawn() -> (String, tokio::task::JoinHandle<anyhow::Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let state = InspectionState::default();
        let handle = tokio::spawn(async move { serve_inspection(listener, state).await });
        (base, handle)
    }

    #[tokio::test]
    async fn preview_returns_stub_shape_with_disabled_flag() {
        let (base, handle) = spawn().await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{base}/api/v1/action/preview"))
            .json(&serde_json::json!({"action_id": "open_logs", "args": {}}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), ReqwStatus::OK);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body.get("enabled").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            body.get("disabled_reason").and_then(|v| v.as_str()),
            Some("preview not yet wired")
        );
        handle.abort();
    }

    #[tokio::test]
    async fn execute_returns_202_with_run_id() {
        let (base, handle) = spawn().await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{base}/api/v1/action/execute"))
            .json(&serde_json::json!({
                "action_id": "open_logs",
                "args": {},
                "idempotency_key": "k-1"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), ReqwStatus::ACCEPTED);
        let body: serde_json::Value = resp.json().await.unwrap();
        let run_id = body.get("run_id").and_then(|v| v.as_str()).unwrap_or("");
        assert!(!run_id.is_empty(), "run_id must be non-empty");
        assert_eq!(
            body.get("status").and_then(|v| v.as_str()),
            Some("accepted")
        );
        handle.abort();
    }

    #[tokio::test]
    async fn stream_returns_text_event_stream() {
        let (base, handle) = spawn().await;
        let resp = reqwest::get(format!("{base}/api/v1/action/run-xyz/stream"))
            .await
            .unwrap();
        assert_eq!(resp.status(), ReqwStatus::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        assert_eq!(ct, "text/event-stream");
        let text = resp.text().await.unwrap();
        assert!(
            text.contains("event: heartbeat"),
            "expected heartbeat event, got: {text}"
        );
        handle.abort();
    }

    #[tokio::test]
    async fn action_registry_returns_non_empty_array() {
        let (base, handle) = spawn().await;
        let resp = reqwest::get(format!("{base}/api/v1/action-registry"))
            .await
            .unwrap();
        assert_eq!(resp.status(), ReqwStatus::OK);
        let body: serde_json::Value = resp.json().await.unwrap();
        let arr = body.as_array().expect("registry must be a JSON array");
        assert!(
            arr.len() >= 30,
            "expected ≥30 registry entries, got {}",
            arr.len()
        );
        // Spot check shape of first entry: must have id + risk_tier.
        let first = &arr[0];
        assert!(first.get("id").and_then(|v| v.as_str()).is_some());
        assert!(first.get("risk_tier").and_then(|v| v.as_str()).is_some());
        handle.abort();
    }
}
