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

use crate::web::action_receipts::{
    ReceiptStatus, WebActionReceipt, compute_state_hash,
};
use crate::web::audit::{RiskTier, write_audit};
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::state::WebState;

// ── Wire DTOs ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PreviewRequest {
    pub action_id: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub params: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ExecuteRequest {
    pub action_id: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub params: HashMap<String, Value>,
    /// Optional concurrency token from the §35.1.14 step-12 contract.
    /// Callers that fetched the action target observed a state hash; if
    /// supplied, the server compares it against the currently-computed
    /// hash and refuses with `settings_hash_stale` on mismatch.
    #[serde(default)]
    pub expected_state_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PreviewResponse {
    pub action_id: String,
    #[schema(value_type = Object)]
    pub preview: ActionPreview,
    pub event_seq: u64,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExecuteResponse {
    pub action_id: String,
    #[schema(value_type = Object)]
    pub result: ActionResult,
    pub event_seq: u64,
    /// `web_action_receipts.id` minted at step 12 of §35.1.14. Callers
    /// can read this back via the receipts log (debug surface) or surface
    /// it to operators for audit trail navigation.
    pub receipt_id: String,
    /// Hash of the action target's state *before* the executor ran. Set
    /// to whatever value the server computed in step 4 (snapshot phase).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_state_hash: Option<String>,
    /// Hash of the action target's state *after* the executor ran. Phase 4
    /// has no real executor so this typically equals
    /// `expected_state_hash`; once real mutators land it diverges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resulting_state_hash: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/actions/preview",
    request_body = PreviewRequest,
    responses(
        (status = 200, description = "Action preview", body = PreviewResponse),
        (status = 404, description = "Unknown action"),
        (status = 401, description = "Unauthenticated"),
    ),
    tag = "actions",
    security(("session" = []), ("csrf" = [])),
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/actions/execute",
    request_body = ExecuteRequest,
    responses(
        (status = 200, description = "Execution result", body = ExecuteResponse),
        (status = 400, description = "Missing Idempotency-Key"),
        (status = 404, description = "Unknown action"),
        (status = 401, description = "Unauthenticated"),
    ),
    tag = "actions",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn execute_action(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    headers: HeaderMap,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, ApiError> {
    // §35.1.14 step 1-2: resolve action, require Idempotency-Key.
    let idem = require_idempotency_key(&headers)?;
    let entry = lookup_action(&req.action_id)?;
    let risk = web_risk_for(entry.risk_tier);

    // §35.1.14 step 3: idempotency replay — return cached result for the
    // same key, skipping the executor.
    let cache_key = format!("action:{}:{}", entry.id, idem);
    if let Some(stored) = state.idempotency.find(&cache_key)
        && let Ok(resp) = serde_json::from_value::<ExecuteResponse>(stored)
    {
        return Ok(Json(resp));
    }

    // §35.1.14 step 4: compute current state hash for the action target.
    // Phase 4 has no executor; the "state" we hash is the registry entry
    // (id, label, risk) plus the caller-supplied params. Once real
    // executors land, replace this with a service lookup
    // (`state.repo_service.get(...)`, etc.).
    let params_value = serde_json::to_value(&req.params).unwrap_or(Value::Null);
    let snapshot_value = serde_json::json!({
        "action_id": entry.id,
        "risk_tier": risk.label(),
        "params": params_value,
    });
    let expected_state_hash =
        compute_state_hash(entry.id, &snapshot_value);

    // §35.1.14 step 5: concurrency check. Only High-risk actions are
    // *required* to send the token, but if any caller supplies one we
    // honor it.
    if let Some(caller_hash) = req.expected_state_hash.as_deref()
        && caller_hash != expected_state_hash
    {
        return Err(ApiError::SettingsHashStale);
    }

    // §35.1.14 step 6-10: execute. Phase 4 has no runtime executor for
    // every action, so we mark `Accepted` and record provider calls as
    // empty. Real executors plug in here.
    let preview = preview_from_entry(entry);
    let result = ActionResult {
        status: ActionStatus::Accepted,
        summary: format!("queued {}", entry.label),
        event_cursor: None,
        affected_entity: None,
        evidence_created: Vec::new(),
    };

    // §35.1.14 step 11: compute resulting state hash. Phase 4 executor is
    // a no-op so the resulting hash equals the expected hash; real
    // mutating executors will diverge once they land.
    let resulting_state_hash = expected_state_hash.clone();

    // §35.1.14 step 12: write the receipt + audit row. Phase 4 ships an
    // in-memory ring + JSONL stamp; durable SQLite lands once the engine
    // pool is plumbed through `WebState`.
    let receipt = WebActionReceipt::new(
        viewer.login.clone(),
        entry.id,
        "action",
        entry.id,
        Some(idem.clone()),
        Some(expected_state_hash.clone()),
        Some(resulting_state_hash.clone()),
        risk,
        ReceiptStatus::Accepted,
    );
    let receipt = state.action_receipts.record(receipt);

    // §35.1.14 step 13: publish on the event bus + audit hook.
    let scope = "global.activity".to_string();
    let payload = serde_json::json!({
        "action_id": entry.id,
        "actor": viewer.login,
        "params": req.params,
        "preview": preview,
        "receipt_id": receipt.id,
        "expected_state_hash": expected_state_hash,
        "resulting_state_hash": resulting_state_hash,
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
        risk,
        payload,
    )
    .await;

    // §35.1.14 step 14: return result + receipt ID for callers.
    let resp = ExecuteResponse {
        action_id: entry.id.to_string(),
        result,
        event_seq,
        receipt_id: receipt.id,
        expected_state_hash: Some(expected_state_hash),
        resulting_state_hash: Some(resulting_state_hash),
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

    #[test]
    fn execute_request_deserializes_with_expected_state_hash() {
        let body = serde_json::json!({
            "action_id": "open_logs",
            "params": {"job_id": "42"},
            "expected_state_hash": "sha256:abc",
        });
        let req: ExecuteRequest =
            serde_json::from_value(body).expect("deserializes");
        assert_eq!(req.action_id, "open_logs");
        assert_eq!(req.expected_state_hash.as_deref(), Some("sha256:abc"));
    }

    #[test]
    fn execute_request_tolerates_missing_expected_state_hash() {
        let body = serde_json::json!({
            "action_id": "open_logs",
            "params": {},
        });
        let req: ExecuteRequest =
            serde_json::from_value(body).expect("deserializes");
        assert!(req.expected_state_hash.is_none());
    }

    #[test]
    fn execute_response_roundtrips_receipt_id() {
        let resp = ExecuteResponse {
            action_id: "open_logs".into(),
            result: ActionResult {
                status: ActionStatus::Accepted,
                summary: "queued Open job logs".into(),
                event_cursor: None,
                affected_entity: None,
                evidence_created: Vec::new(),
            },
            event_seq: 7,
            receipt_id: "rcpt_abc".into(),
            expected_state_hash: Some("sha256:before".into()),
            resulting_state_hash: Some("sha256:after".into()),
        };
        let v = serde_json::to_value(&resp).expect("serializes");
        assert_eq!(v["receipt_id"], "rcpt_abc");
        assert_eq!(v["expected_state_hash"], "sha256:before");
        assert_eq!(v["resulting_state_hash"], "sha256:after");
        let back: ExecuteResponse =
            serde_json::from_value(v).expect("deserializes");
        assert_eq!(back.receipt_id, "rcpt_abc");
    }
}
