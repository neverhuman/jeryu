use super::*;
use crate::api::entity::{EntityKind, EntityRef};

fn body_for(response: InspectionHttpResponse) -> serde_json::Value {
    serde_json::from_str(&response.body).expect("valid json")
}

#[test]
fn preview_requires_grant_for_mutating_action() {
    let response = handle_post("/api/v1/actions/requeue_job/preview", b"{}").unwrap();
    assert_eq!(response.status, 200);
    let body = body_for(response);
    assert_eq!(body["data"]["enabled"], false);
    assert_eq!(body["data"]["required_grant"], "agent_task");
}

#[test]
fn execute_rejects_missing_idempotency_key() {
    let response = handle_post("/api/v1/actions/run_tests/execute", b"{}").unwrap();
    assert_eq!(response.status, 400);
    assert!(response.body.contains("idempotency key is required"));
}

#[test]
fn dry_run_execute_returns_stable_receipt() {
    let request = serde_json::to_vec(&ActionOperationRequest {
        selected_entity: Some(EntityRef::new(EntityKind::MergeRequest, "42")),
        grant_id: Some("grant-1".into()),
        dry_run: true,
        idempotency_key: Some("same-key".into()),
        ..Default::default()
    })
    .unwrap();

    let first = body_for(handle_post("/api/v1/actions/run_tests/execute", &request).unwrap());
    let second = body_for(handle_post("/api/v1/actions/run_tests/execute", &request).unwrap());

    assert_eq!(first["data"]["result"]["status"], "completed");
    assert_eq!(
        first["data"]["receipt"]["receipt_id"],
        second["data"]["receipt"]["receipt_id"]
    );
    assert_eq!(first["data"]["stream"]["events"][0]["phase"], "execute");
}

#[test]
fn cancel_returns_cancelled_receipt() {
    let request = br#"{"idempotency_key":"run-1"}"#;
    let body = body_for(handle_post("/api/v1/actions/run_tests/cancel", request).unwrap());
    assert_eq!(body["data"]["result"]["status"], "cancelled");
    assert_eq!(body["data"]["stream"]["events"][0]["phase"], "cancel");
}

#[test]
fn action_stream_route_returns_empty_page() {
    let response = handle_get("/api/v1/action-stream");
    let body = body_for(response);
    assert_eq!(body["cursor"], 0);
    assert!(body["events"].as_array().unwrap().is_empty());
}
