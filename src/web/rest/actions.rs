//! REST handlers for generic action preview/execute (W-B-15).
//!
//! Exposes (per §35.7):
//!   - `POST /api/v1/actions/preview`               compute `ActionPreview`
//!   - `POST /api/v1/actions/execute`  (Idempotency-Key)
//!
//! These wrap the canonical `api::actions` action registry. Phase 4 does
//! NOT yet have a runtime executor for every action — the handler returns
//! a placeholder preview/result, writes an audit event, and publishes
//! `action.previewed` / `action.executed` on the WebSocket bus. Real
//! mutation lands when the per-action executors ship (see §35.1.14).

use std::collections::HashMap;

use axum::{
    Extension, Json,
    extract::State,
    http::HeaderMap,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use jeryu::api::actions::{ActionPreview, ActionResult, ActionStatus};
use jeryu::api::websocket::WebEvent;
use jeryu::tui::action_registry::{ActionEntry, REGISTRY};

use crate::web::audit::{RiskTier, write_audit};
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::state::WebState;

// ── Wire DTOs ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub action_id: String,
    #[serde(default)]
    pub params: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    pub action_id: String,
    #[serde(default)]
    pub params: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreviewResponse {
    pub action_id: String,
    pub preview: ActionPreview,
    pub event_seq: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub action_id: String,
    pub result: ActionResult,
    pub event_seq: u64,
}

// ── Handlers ──────────────────────────────────────────────────────────

pub async fn preview_action(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Json(req): Json<PreviewRequest>,
) -> Result<Json<PreviewResponse>, ApiError> {
    let entry = lookup_action(&req.action_id)?;
    let preview = preview_from_entry(entry);
    let scope = "global.activity".to_string();
    let payload = serde_json::json!({
        "action_id": entry.id,
        "actor": viewer.login,
        "params": req.params,
    });
    let event_seq = state.event_bus.publish(WebEvent {
        seq: 0,
        timestamp: Utc::now(),
        scope,
        kind: "action.previewed".into(),
        entity: format!("action:{}", entry.id),
        summary: format!("preview {}", entry.label),
        payload: payload.clone(),
    });
    write_audit(
        &viewer.login,
        &format!("action.preview:{}", entry.id),
        &format!("action:{}", entry.id),
        web_risk_for(entry.risk_tier),
        payload,
    )
    .await;
    Ok(Json(PreviewResponse {
        action_id: entry.id.to_string(),
        preview,
        event_seq,
    }))
}

pub async fn execute_action(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    headers: HeaderMap,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, ApiError> {
    let idem = require_idempotency_key(&headers)?;
    let entry = lookup_action(&req.action_id)?;

    let cache_key = format!("action:{}:{}", entry.id, idem);
    if let Some(stored) = state.idempotency.find(&cache_key) {
        if let Ok(resp) = serde_json::from_value::<ExecuteResponse>(stored) {
            return Ok(Json(resp));
        }
    }

    // Phase 4: no runtime executor; mark as Accepted, publish an event, audit.
    let preview = preview_from_entry(entry);
    let result = ActionResult {
        status: ActionStatus::Accepted,
        summary: format!("queued {}", entry.label),
        event_cursor: None,
        affected_entity: None,
        evidence_created: Vec::new(),
    };
    let scope = "global.activity".to_string();
    let payload = serde_json::json!({
        "action_id": entry.id,
        "actor": viewer.login,
        "params": req.params,
        "preview": preview,
    });
    let event_seq = state.event_bus.publish(WebEvent {
        seq: 0,
        timestamp: Utc::now(),
        scope,
        kind: "action.executed".into(),
        entity: format!("action:{}", entry.id),
        summary: format!("executed {}", entry.label),
        payload: payload.clone(),
    });
    write_audit(
        &viewer.login,
        &format!("action.execute:{}", entry.id),
        &format!("action:{}", entry.id),
        web_risk_for(entry.risk_tier),
        payload,
    )
    .await;
    let resp = ExecuteResponse {
        action_id: entry.id.to_string(),
        result,
        event_seq,
    };
    state
        .idempotency
        .store(cache_key, serde_json::to_value(&resp).unwrap_or_default());
    Ok(Json(resp))
}

// ── Helpers ──────────────────────────────────────────────────────────

fn lookup_action(action_id: &str) -> Result<&'static ActionEntry, ApiError> {
    REGISTRY
        .iter()
        .find(|e| e.id == action_id)
        .ok_or_else(|| ApiError::NotFound(format!("action: {action_id}")))
}

fn preview_from_entry(entry: &ActionEntry) -> ActionPreview {
    ActionPreview {
        enabled: true,
        disabled_reason: None,
        risk: entry.risk_tier,
        side_effect_class: entry.side_effect_class(),
        side_effects: vec![entry.description.to_string()],
        will_not: Vec::new(),
        summary: entry.label.to_string(),
        evidence_expected: Vec::new(),
        required_grant: entry.required_grant(),
        undo_action: None,
        confirm_prompt: None,
    }
}

fn web_risk_for(tier: jeryu::tui::action_registry::RiskTier) -> RiskTier {
    use jeryu::tui::action_registry::RiskTier as R;
    match tier {
        R::R0 => RiskTier::Low,
        R::R1 | R::R2 => RiskTier::Medium,
        R::R3 | R::R4 | R::R5 => RiskTier::High,
    }
}

fn require_idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let v = headers
        .get("idempotency-key")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::BadRequest("Idempotency-Key header is required".into()))?;
    Ok(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_finds_registered_action() {
        let entry = lookup_action("open_logs").expect("open_logs registered");
        assert_eq!(entry.id, "open_logs");
    }

    #[test]
    fn lookup_returns_not_found_for_unknown() {
        let err = lookup_action("nope").unwrap_err();
        assert!(matches!(err, ApiError::NotFound(_)));
    }

    #[test]
    fn preview_uses_entry_metadata() {
        let entry = lookup_action("open_logs").unwrap();
        let preview = preview_from_entry(entry);
        assert!(preview.enabled);
        assert_eq!(preview.summary, "Open job logs");
    }
}
