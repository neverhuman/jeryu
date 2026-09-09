//! Cell-surface REST + error-path coverage for the workcell endpoints.
//!
//! These exercise the previously-untested surface handlers — `list` / `status` /
//! `claim` / `heartbeat` — plus the 404 / epoch-fence / malformed-body /
//! id-mismatch error paths, by driving the handlers directly (and the live
//! router once, to prove the route is wired). The whole `web` module is
//! `#[cfg(feature = "web")]`-gated, so this submodule compiles to nothing without
//! `--features web` (an honest skip by cfg). `tests.rs`'s helpers are not visible
//! from a sibling module, so `response_json` is inlined.

use super::*;
use serde_json::{Value, json};

/// Decode an axum response body into JSON. Inlined from `tests.rs`, whose helpers
/// live in a sibling module and so cannot be imported here.
async fn response_json(response: AxumResponse) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body reads");
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("response body is not JSON ({err}): {bytes:?}"))
}

/// A claim body that drives a Ready->Claimed cell via a Rebased startup (no git).
fn claim_body(epoch: u64) -> Value {
    json!({
        "agent_id": "agent-1",
        "workspace_root": "/ws",
        "repo_roots": ["/ws"],
        "branch_budget": 1,
        "runner_id": "r1",
        "runner_epoch": epoch,
        "git_status_summary": "clean",
        "ci_snapshot_age_ms": 0,
        "startup": {
            "state": "rebased",
            "main_ref": "origin/main",
            "base_sha": "aaa",
            "head_sha": "bbb"
        }
    })
}

#[tokio::test]
async fn claim_rewrites_retired_jekko_repo_root() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let mut body = claim_body(7);
    body["workspace_root"] = json!("/home/ubuntu/jekko");
    body["repo_roots"] = json!(["/home/ubuntu/jekko/jnoccio-fusion"]);

    let claimed = response_json(
        super::workcells::claim(
            State(state),
            axum::body::Bytes::from(serde_json::to_vec(&body).unwrap()),
        )
        .await,
    )
    .await;

    assert_eq!(claimed["state"], "claimed");
    assert_eq!(
        claimed["workspace_root"],
        json!("/home/ubuntu/jekko-split/jekko")
    );
    assert_eq!(
        claimed["repo_roots"],
        json!(["/home/ubuntu/jekko-split/jekko/jnoccio-fusion"])
    );
}

#[tokio::test]
async fn list_is_empty_then_reflects_claim_status_and_heartbeat() {
    let state = Arc::new(WebState::new(ForgeCore::new()));

    // The list starts empty.
    let empty = super::workcells::list(State(state.clone())).await;
    assert!(empty.0.is_empty(), "a fresh manager exposes no workcells");

    // Claim a ready cell: state becomes "claimed" and the epoch round-trips.
    let claimed = response_json(
        super::workcells::claim(
            State(state.clone()),
            axum::body::Bytes::from(serde_json::to_vec(&claim_body(3)).unwrap()),
        )
        .await,
    )
    .await;
    assert_eq!(claimed["state"], "claimed");
    assert_eq!(claimed["runner_epoch"], 3);
    let id = claimed["workcell_id"]
        .as_str()
        .expect("claim returns a workcell id")
        .to_string();

    // The list now contains the claimed cell (alongside any warm replacement the
    // manager keeps ready — the exact pool size is an implementation detail).
    let listed = super::workcells::list(State(state.clone())).await;
    assert!(
        listed.0.iter().any(|w| w.workcell_id == id),
        "the claimed cell must appear in the workcell list (got {} cells)",
        listed.0.len()
    );

    // GET :id status returns the same lease.
    let status =
        response_json(super::workcells::status(State(state.clone()), AxumPath(id.clone())).await)
            .await;
    assert_eq!(status["workcell_id"], id.as_str());
    assert_eq!(status["runner_epoch"], 3);

    // A heartbeat with the matching epoch keeps the cell healthy.
    let hb = response_json(
        super::workcells::heartbeat(
            State(state.clone()),
            AxumPath(id.clone()),
            axum::body::Bytes::from(
                serde_json::to_vec(&json!({ "runner_epoch": 3, "heartbeat_healthy": true }))
                    .unwrap(),
            ),
        )
        .await,
    )
    .await;
    // The heartbeat returns the refreshed lease: same cell, same epoch, healthy.
    assert_eq!(hb["heartbeat_healthy"], true);
    assert_eq!(hb["runner_epoch"], 3);
    assert_eq!(hb["workcell_id"], id.as_str());
}

#[tokio::test]
async fn status_of_unknown_cell_is_typed_404() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let resp = super::workcells::status(State(state), AxumPath("wc-nope".into())).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = response_json(resp).await;
    assert_eq!(body["code"], "not_found");
    assert_eq!(body["purpose"], "inspect a workcell");
    for k in ["reason", "common_fixes", "docs_url", "repair_hint"] {
        assert!(body.get(k).is_some(), "typed 404 must carry `{k}`");
    }
}

#[tokio::test]
async fn heartbeat_with_stale_epoch_is_fenced_409() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let claimed = response_json(
        super::workcells::claim(
            State(state.clone()),
            axum::body::Bytes::from(serde_json::to_vec(&claim_body(5)).unwrap()),
        )
        .await,
    )
    .await;
    let id = claimed["workcell_id"].as_str().unwrap().to_string();

    // A heartbeat carrying the wrong runner epoch is fenced (409), not accepted.
    let resp = super::workcells::heartbeat(
        State(state.clone()),
        AxumPath(id),
        axum::body::Bytes::from(serde_json::to_vec(&json!({ "runner_epoch": 999 })).unwrap()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(response_json(resp).await["code"], "workcell_epoch_fenced");
}

#[tokio::test]
async fn claim_with_malformed_json_is_typed_422() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let resp =
        super::workcells::claim(State(state), axum::body::Bytes::from_static(b"{ not json")).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(resp).await["code"],
        "workcell_invalid_request"
    );
}

#[tokio::test]
async fn export_pr_with_mismatched_id_is_400_before_any_git() {
    // The path id and the body's workcell_id disagree -> 400, short-circuited
    // before any cell lookup or git diff (so no claim/git setup is needed).
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let body = json!({
        "workcell_id": "wc-other",
        "runner_epoch": 4,
        "branch_suffix": "s",
        "owner": "alice",
        "repo": "jeryu",
        "author": "agent-1"
    });
    let resp = super::workcells::export_pr(
        State(state),
        AxumPath("wc-real".into()),
        axum::http::HeaderMap::new(),
        axum::body::Bytes::from(serde_json::to_vec(&body).unwrap()),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response_json(resp).await["code"], "workcell_id_mismatch");
}

#[tokio::test]
async fn list_route_is_wired_and_returns_empty_array() {
    use axum::body::Body;
    use tower::ServiceExt;
    let resp = app(
        WebState::new(ForgeCore::new()),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    )
    .oneshot(
        Request::builder()
            .uri("/api/v1/workcells")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(response_json(resp).await.as_array().unwrap().len(), 0);
}
