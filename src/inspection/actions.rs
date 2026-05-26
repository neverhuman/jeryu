//! Owner: Inspection API - action preview and execution contracts
//! Proof: `cargo test -p jeryu --lib inspection::actions`
//! Invariants: inspection actions are typed and never bypass the action registry.

use chrono::Utc;

use crate::api::actions::{
    ActionExecutionResponse, ActionOperationRequest, ActionPreview, ActionReceipt, ActionResult,
    ActionStatus, ActionStreamEvent, ActionStreamPage, ActionStreamPhase,
};
use crate::api::inspection::InspectionEnvelope;
use crate::tui::action_registry::{ActionEntry, GrantRequirement, REGISTRY, SideEffectClass};

use super::{InspectionHttpResponse, inspection_sources};

pub fn is_action_get(path: &str) -> bool {
    path == "/api/v1/action-stream"
        || (path.starts_with("/api/v1/actions/") && path.ends_with("/stream"))
}

pub fn handle_get(path: &str) -> InspectionHttpResponse {
    if path == "/api/v1/action-stream" || path.starts_with("/api/v1/actions/") {
        return InspectionHttpResponse::json(200, &ActionStreamPage::empty(0));
    }
    not_found("unknown action stream route")
}

pub fn handle_post(path: &str, body: &[u8]) -> Option<InspectionHttpResponse> {
    let rest = path.strip_prefix("/api/v1/actions/")?;
    let (action_id, op) = rest.split_once('/')?;
    if action_id.is_empty() {
        return Some(not_found("action id is required"));
    }
    let Some(entry) = find_action(action_id) else {
        return Some(not_found("unknown action id"));
    };

    let request = match parse_request(body) {
        Ok(request) => request,
        Err(response) => return Some(response),
    };

    let response = match op {
        "preview" => preview_response(entry, &request),
        "execute" => execute_response(entry, &request),
        "cancel" => cancel_response(entry, &request),
        _ => not_found("unknown action operation"),
    };
    Some(response)
}

fn preview_response(
    entry: &ActionEntry,
    request: &ActionOperationRequest,
) -> InspectionHttpResponse {
    let now = Utc::now();
    let preview = build_preview(entry, request);
    InspectionHttpResponse::json(
        200,
        &InspectionEnvelope::new(preview, inspection_sources(now), now),
    )
}

fn execute_response(
    entry: &ActionEntry,
    request: &ActionOperationRequest,
) -> InspectionHttpResponse {
    let Some(key) = request.idempotency_key.as_deref() else {
        return error(400, "idempotency key is required");
    };

    let preview = build_preview(entry, request);
    if !preview.enabled {
        let result = result(
            entry.id,
            ActionStatus::Rejected,
            preview
                .disabled_reason
                .clone()
                .unwrap_or_else(|| "action preconditions failed".into()),
            request,
            None,
        );
        return execution_response(
            entry.id,
            key,
            request.dry_run,
            result,
            ActionStreamPhase::Execute,
            403,
        );
    }

    let status = if entry.side_effect_class() == SideEffectClass::ReadOnly || request.dry_run {
        ActionStatus::Completed
    } else {
        ActionStatus::RequiresApproval
    };
    let summary = match status {
        ActionStatus::Completed if request.dry_run => {
            format!("dry-run accepted for action '{}'", entry.id)
        }
        ActionStatus::Completed => format!("read-only action '{}' completed", entry.id),
        ActionStatus::RequiresApproval => format!(
            "action '{}' is gated; submit required proof before live execution",
            entry.id
        ),
        _ => format!("action '{}' did not execute", entry.id),
    };
    let result = result(entry.id, status, summary, request, Some(key));
    execution_response(
        entry.id,
        key,
        request.dry_run,
        result,
        ActionStreamPhase::Execute,
        202,
    )
}

fn cancel_response(
    entry: &ActionEntry,
    request: &ActionOperationRequest,
) -> InspectionHttpResponse {
    let Some(key) = request.idempotency_key_or_run_id() else {
        return error(400, "idempotency key or action run id is required");
    };
    let result = result(
        entry.id,
        ActionStatus::Cancelled,
        format!("action '{}' cancellation recorded", entry.id),
        request,
        Some(key),
    );
    execution_response(
        entry.id,
        key,
        request.dry_run,
        result,
        ActionStreamPhase::Cancel,
        202,
    )
}

fn build_preview(entry: &ActionEntry, request: &ActionOperationRequest) -> ActionPreview {
    let required_grant = entry.required_grant();
    let grant_missing = required_grant != GrantRequirement::None && request.grant_id.is_none();
    ActionPreview {
        enabled: !grant_missing,
        disabled_reason: grant_missing.then(|| {
            format!(
                "grant '{}' is required before execution",
                required_grant.label()
            )
        }),
        risk: entry.risk_tier,
        side_effect_class: entry.side_effect_class(),
        side_effects: side_effects(entry),
        will_not: vec![
            "mutate live systems during preview".into(),
            "execute without idempotency and grants".into(),
        ],
        summary: format!("Preview for action '{}': {}", entry.id, entry.description),
        evidence_expected: vec![format!("action_receipt:{}", entry.id)],
        required_grant,
        undo_action: None,
        confirm_prompt: (entry.confirmation_policy().label() != "none")
            .then(|| entry.confirmation_policy().label().to_string()),
    }
}

fn result(
    action_id: &str,
    status: ActionStatus,
    summary: String,
    request: &ActionOperationRequest,
    key: Option<&str>,
) -> ActionResult {
    let evidence_created = match key {
        Some(k) => vec![crate::api::actions::receipt_id(action_id, k)],
        None => Vec::new(),
    };
    ActionResult {
        status,
        summary,
        event_cursor: key.map(crate::api::actions::cursor_for_key),
        affected_entity: request.selected_entity.clone(),
        evidence_created,
    }
}

fn execution_response(
    action_id: &str,
    key: &str,
    dry_run: bool,
    result: ActionResult,
    phase: ActionStreamPhase,
    status: u16,
) -> InspectionHttpResponse {
    let now = Utc::now();
    let receipt = ActionReceipt::from_result(action_id, key, dry_run, &result, now);
    let stream = ActionStreamPage::single(ActionStreamEvent {
        seq: result
            .event_cursor
            .unwrap_or_else(|| crate::api::actions::cursor_for_key(key)),
        action_id: action_id.to_string(),
        phase,
        status: result.status,
        summary: result.summary.clone(),
        receipt_id: Some(receipt.receipt_id.clone()),
        timestamp: now,
    });
    InspectionHttpResponse::json(
        status,
        &InspectionEnvelope::new(
            ActionExecutionResponse {
                result,
                receipt,
                stream,
            },
            inspection_sources(now),
            now,
        ),
    )
}

fn side_effects(entry: &ActionEntry) -> Vec<String> {
    match entry.side_effect_class() {
        SideEffectClass::ReadOnly => vec!["read current projection state".into()],
        SideEffectClass::LocalState => vec!["modify local control-plane state".into()],
        SideEffectClass::GitWrite => vec!["write git branch or merge-request state".into()],
        SideEffectClass::CiExecution => vec!["start or retry CI work".into()],
        SideEffectClass::Merge => vec!["request or perform merge flow".into()],
        SideEffectClass::Production => vec!["touch production release state".into()],
    }
}

fn parse_request(body: &[u8]) -> Result<ActionOperationRequest, InspectionHttpResponse> {
    if body.is_empty() {
        return Ok(ActionOperationRequest::default());
    }
    serde_json::from_slice(body).map_err(|err| {
        error(
            400,
            &format!(
                "invalid action request json: {}",
                err.to_string().replace('"', "'")
            ),
        )
    })
}

fn find_action(action_id: &str) -> Option<&'static ActionEntry> {
    REGISTRY.iter().find(|entry| entry.id == action_id)
}

fn not_found(message: &str) -> InspectionHttpResponse {
    error(404, message)
}

fn error(status: u16, message: &str) -> InspectionHttpResponse {
    InspectionHttpResponse::json(status, &serde_json::json!({ "error": message }))
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
