//! W-F-13 — Regression guard for the legacy engine routes.
//!
//! Per `WEB_WORK_CLAUDE.md` §35.1.5, the JeRyu engine has historically exposed
//! three HTTP routes:
//!
//! * `GET  /health`
//! * `POST /hooks`
//! * `GET  /cache/summary`
//!
//! When the Web Forge BFF lands (work packages W-B-01..W-B-04), the router
//! merge **must not** drop or rebind any of these endpoints — operators rely
//! on them (`/health` is wired into uptime probes; `/hooks` is the GitLab
//! webhook receiver; `/cache/summary` powers the cache TUI dashboards).
//!
//! This integration test exercises `jeryu::engine::build_router` directly and
//! asserts that none of the three paths return `404 NOT FOUND`. The handlers
//! themselves short-circuit on missing auth headers (we deliberately omit the
//! `X-Gitlab-Token` / `X-Jeryu-Token` headers so the deeper state-touching
//! code paths never fire) — the test only cares that the routes remain
//! *registered*. Future work packages that intentionally retire one of these
//! routes must update this test and call that out in the §35.1.5 contract.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

use jeryu::docker::DockerCtl;
use jeryu::engine::{self, EngineState};
use jeryu::gitlab_client::GitlabClient;
use jeryu::runner_backend_registry::BackendRegistry;
use jeryu::state::Db;

/// Construct a minimal `EngineState` suitable for router-only tests. The state
/// is intentionally bare: an in-memory SQLite DB, a disconnected Docker client,
/// and a local-only backend registry. None of the test paths reach the DB or
/// Docker socket because each handler we hit short-circuits on the missing
/// auth header (or, in `/health`'s case, ignores state entirely).
async fn build_test_state() -> Arc<EngineState> {
    let db = Db::open_memory()
        .await
        .expect("Db::open_memory() should succeed for an isolated in-memory DB");
    let docker = DockerCtl::disconnected();
    let client = GitlabClient::new("http://127.0.0.1:0", None);
    let backend_registry = BackendRegistry::local_only(docker.clone());
    Arc::new(EngineState {
        db,
        docker,
        client,
        webhook_secret: "test-secret-not-used".to_string(),
        backend_registry,
        node_gc_timestamps: Mutex::new(HashMap::new()),
    })
}

/// Issue a request and return the response status, asserting we did NOT get a
/// 404 (which is axum's signal that the route is missing from the router).
async fn assert_route_bound(method: Method, path: &'static str) {
    let state = build_test_state().await;
    let app = engine::build_router(state);

    let req = Request::builder()
        .method(method.clone())
        .uri(path)
        .body(Body::empty())
        .expect("test request builds");

    let res = app.oneshot(req).await.expect("oneshot delivers a response");

    assert_ne!(
        res.status(),
        StatusCode::NOT_FOUND,
        "legacy engine route `{method} {path}` was removed from the router — \
this violates WEB_WORK_CLAUDE.md §35.1.5 (W-B-02 router merge must preserve \
/health, /hooks, /cache/summary). If you intentionally retired the route, \
update tests/engine_routes_preserved_test.rs and document the change in \
§35.1.5.",
        method = method,
        path = path,
    );
}

#[tokio::test(flavor = "current_thread")]
async fn legacy_health_route_remains_bound() {
    assert_route_bound(Method::GET, "/health").await;
}

#[tokio::test(flavor = "current_thread")]
async fn legacy_hooks_route_remains_bound() {
    assert_route_bound(Method::POST, "/hooks").await;
}

#[tokio::test(flavor = "current_thread")]
async fn legacy_cache_summary_route_remains_bound() {
    assert_route_bound(Method::GET, "/cache/summary").await;
}
