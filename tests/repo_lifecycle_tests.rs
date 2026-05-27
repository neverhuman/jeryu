//! W-T-02 · Repo lifecycle service tests
//!
//! Covers the §35.1.18 dry-run preview / idempotent execute / event-emit
//! contract for `RepoService::preview_create` and `RepoService::create`.
//!
//! Phase-2 status (as of `web-forge/main` @ c1dadbb): `src/repos/` is NOT
//! yet present. The W-P2-backend agent is still composing
//! `RepoService::preview_create` / `RepoService::create` and the
//! `FakeGitlab` (`GitHost` mock) it will be wired against. Every test in
//! this file is therefore marked `#[ignore]` with a clear reason; the
//! orchestrator un-ignores once W-P2-backend merges into web-forge/main.
//!
//! What IS exercised today: the publish/subscribe surface of
//! `WebEventBus` (already in `web-forge/main`), as a forward-compatibility
//! check that the bus contract the service layer will consume keeps
//! routing high/medium events to the High channel and exposing a
//! monotonic seq the way the service plan §35.7 expects.

#![cfg(feature = "web")]

use std::sync::Arc;

use chrono::Utc;
use jeryu::api::websocket::WebEvent;
use jeryu::web_events::WebEventBus;

// ---------------------------------------------------------------------------
// Forward-compatibility coverage: the WebEventBus surface that
// RepoService::create will publish onto.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn web_event_bus_routes_repo_created_to_high_channel() {
    // RepoService::create publishes `repo.created` (High priority per
    // §35.1.13). Verify the bus still routes that kind through the High
    // subscriber, since that is the surface the service test below will
    // rely on once it un-ignores.
    let bus = Arc::new(WebEventBus::with_defaults());
    let mut rx = bus.subscribe_high();

    let seq = bus.publish(WebEvent {
        seq: 0,
        timestamp: Utc::now(),
        scope: "global.activity".into(),
        kind: "repo.created".into(),
        entity: "repo:demo".into(),
        summary: "demo repo created".into(),
        payload: serde_json::json!({}),
    });

    assert_eq!(seq, 1, "first publish must allocate seq=1");
    let evt = rx
        .try_recv()
        .expect("repo.created must arrive on the high channel");
    assert_eq!(evt.kind, "repo.created");
    assert_eq!(evt.seq, 1);
}

// ---------------------------------------------------------------------------
// Pending tests — un-ignored once RepoService lands in web-forge/main.
// Each retains the §35.7 contract its name describes so the orchestrator
// can run `cargo nextest run -- --include-ignored` to validate the wired
// service once W-P2-backend merges.
// ---------------------------------------------------------------------------

/// W-T-02 · dry_run preview path
///
/// `RepoService::preview_create({ dry_run: true, ... })` must:
///   1. Return a fully-populated preview body (initial files, refs).
///   2. Not write any row to `web_repositories` or `audit_events`.
///   3. Not invoke `GitHost::create_repository` on the underlying host.
#[tokio::test]
#[ignore = "W-P2-backend: RepoService::preview_create not yet in web-forge/main; un-ignore when W-P2-backend lands"]
async fn dry_run_create_does_not_write() {
    // TODO(W-P2-backend): wire RepoService + FakeGitlab (impl GitHost) and assert:
    //   - preview returned with initial_files containing README.md
    //   - audit/event tables empty after the call
    //   - FakeGitlab::create_repository never invoked
    panic!("pending W-P2-backend RepoService::preview_create");
}

/// W-T-02 · idempotent execute path
///
/// `RepoService::create({ idempotency_key })` replayed with the same key:
///   - First call writes once and emits `repo.created`.
///   - Second call returns the cached response (no second host write,
///     no second event, no second audit row) — §35.1.16.
#[tokio::test]
#[ignore = "W-P2-backend: RepoService::create idempotency not yet in web-forge/main"]
async fn create_writes_once_with_idempotency_key() {
    // TODO(W-P2-backend):
    //   1. let svc = RepoService::with_fake_gitlab();
    //   2. let key = "idem-key-001";
    //   3. let a = svc.create(CreateRepoInput { idempotency_key: key, ... }).await?;
    //   4. let b = svc.create(CreateRepoInput { idempotency_key: key, ... }).await?;
    //   5. assert_eq!(a.repo.id, b.repo.id);
    //   6. assert_eq!(fake_gitlab.create_calls(), 1);
    //   7. assert_eq!(events_emitted("repo.created"), 1);
    panic!("pending W-P2-backend RepoService::create");
}

/// W-T-02 · event emission
///
/// `RepoService::create` publishes a `WebEvent { kind: "repo.created", ... }`
/// to the shared `WebEventBus`; a subscriber receives it.
#[tokio::test]
#[ignore = "W-P2-backend: RepoService::create event emission not yet wired"]
async fn create_emits_repo_created_event() {
    // TODO(W-P2-backend):
    //   1. let bus = Arc::new(WebEventBus::with_defaults());
    //   2. let mut rx = bus.subscribe_high();
    //   3. let svc = RepoService::with_bus(bus.clone());
    //   4. svc.create(CreateRepoInput { dry_run: false, ... }).await?;
    //   5. let evt = rx.try_recv()?;
    //   6. assert_eq!(evt.kind, "repo.created");
    //   7. assert!(evt.entity.starts_with("repo:"));
    panic!("pending W-P2-backend RepoService::create + WebEventBus wiring");
}
