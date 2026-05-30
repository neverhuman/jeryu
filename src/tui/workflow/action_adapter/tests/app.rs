//! Owner: Interactive TUI subsystem — Mission Control adapter App-level tests (Wave 6.A)
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter::tests::app`
//! Invariants: Exercises App-level wiring incl. production-adapter install + key routing.

// Wave 6.A bin-integration tests.
//
// These bind the action_adapter trait to App-level wiring (default
// FakeActionAdapter, `try_install_production_adapter`, kind() seam).
// They are the contract that `bin/autonomy` (or `tui/runner.rs`) calls
// `try_install_production_adapter` once at startup and the rest of the
// TUI flows through `App::action_adapter` without ever touching the
// concrete GitHubClient / SqlLedger types directly.

use std::collections::HashMap;
use std::sync::Arc;

use super::super::{ActionAdapter, FakeActionAdapter, ProductionActionAdapter, RecordedCall};
use crate::tui::workflow::actions::ActionOutcome;
use crate::tui::workflow::delivery::build_demo_delivery;

async fn app_with_demo_delivery() -> crate::tui::app::App {
    let mut app = crate::tui::app::test_app()
        .await
        .expect("build in-memory test app");
    app.delivery_snapshot = build_demo_delivery();
    app
}

#[tokio::test]
async fn app_default_action_adapter_is_fake() {
    let app = app_with_demo_delivery().await;
    assert_eq!(
        app.action_adapter.kind(),
        "fake",
        "App must default to FakeActionAdapter so unit tests don't need a DB"
    );
}

#[tokio::test]
async fn app_action_pane_key_uses_installed_adapter() {
    // Install a known FakeActionAdapter, route a key through the pane,
    // and verify the recorded call lands on THAT instance — proving the
    // App routes through `self.action_adapter` instead of synthesizing a
    // throw-away adapter per keystroke (the Wave-6.A regression).
    let mut app = app_with_demo_delivery().await;
    let fake = Arc::new(FakeActionAdapter::new());
    // Capture a handle to the inner Mutex<Vec<RecordedCall>> so we can
    // observe calls after the App takes ownership of the Arc.
    let calls_handle = fake.calls.clone();
    app.action_adapter = fake.clone();
    app.action_pane.visible = true;

    // 'A' triggers ApproveOnce on the focused PR (idx 0).
    let consumed = app
        .action_pane_key(crossterm::event::KeyCode::Char('A'))
        .await;
    assert!(consumed, "action pane should consume the key while visible");
    let calls = calls_handle.lock().unwrap().clone();
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, RecordedCall::PostPassportCheck { .. })),
        "the installed adapter must record the ApproveOnce passport check; got {calls:?}",
    );
}

#[tokio::test]
async fn try_install_production_adapter_succeeds_when_secrets_resolve() {
    // `Db::open` memoizes the first successful pool in a global
    // OnceCell. Use a tempfile-backed SQLite DB so the multi-connection
    // pool (`open_url` uses max_connections=4) sees a shared schema
    // after migration, instead of the per-connection isolation that
    // in-memory DB would create. Safe to set even if the
    // singleton was already initialized — the call just reuses the
    // cached pool.
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    let db_url = crate::db::config::sqlite_url(tmp.path());
    let resolver = crate::llm::secrets::SecretResolver {
        env_overrides: HashMap::from([
            (
                "GITHUB_TOKEN".to_string(),
                concat!("ghp_", "test_wave6a_install").to_string(),
            ),
            ("JERYU_DATABASE_URL".to_string(), db_url.clone()),
        ]),
        ..Default::default()
    };

    let mut app = app_with_demo_delivery().await;
    assert_eq!(app.action_adapter.kind(), "fake", "starts as fake");

    let result = app
        .try_install_production_adapter_with_resolver(&resolver)
        .await;

    assert!(
        result.is_ok(),
        "expected production adapter install to succeed: {:?}",
        result.err()
    );
    assert_eq!(
        app.action_adapter.kind(),
        "production",
        "kind() should flip to 'production' after successful install"
    );
}

#[tokio::test]
async fn try_install_production_adapter_keeps_fake_when_token_missing() {
    let mut resolver = crate::llm::secrets::SecretResolver::default();
    resolver
        .env_overrides
        .insert("GITHUB_TOKEN".to_string(), String::new());
    resolver.ci_mode = true;

    let mut app = app_with_demo_delivery().await;
    let result = app
        .try_install_production_adapter_with_resolver(&resolver)
        .await;

    assert!(
        result.is_err(),
        "missing GITHUB_TOKEN must return Err; got Ok"
    );
    let msg = format!("{:?}", result.err().unwrap());
    assert!(
        msg.contains("GITHUB_TOKEN"),
        "error should name the missing secret: {msg}"
    );
    assert_eq!(
        app.action_adapter.kind(),
        "fake",
        "App must keep the fake adapter on failure so the TUI stays usable"
    );
}

#[test]
fn production_adapter_kind_returns_production() {
    // Build a ProductionActionAdapter without going through Db::open so
    // we exercise the `kind()` default impl without env/DB side effects.
    use crate::autonomy::signing::EdSigningKey;
    use crate::db::{AnyPoolOptions, config as db_config, install_default_drivers};
    use crate::git_host::GitHubClient;
    use tempfile::NamedTempFile;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let adapter = rt.block_on(async {
        install_default_drivers();
        let tmp = NamedTempFile::new().expect("tempfile for production adapter test");
        let url = db_config::sqlite_url(tmp.path());
        let pool = AnyPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("file-backed sqlite pool");
        std::mem::forget(tmp);
        ProductionActionAdapter::new(
            Arc::new(GitHubClient::new("ghp_test_kind")),
            pool,
            Arc::new(EdSigningKey::generate("tui.cockpit.v1.test")),
        )
    });
    assert_eq!(adapter.kind(), "production");
}

#[test]
fn fake_adapter_kind_returns_fake() {
    let fake = FakeActionAdapter::new();
    assert_eq!(fake.kind(), "fake");
}

#[test]
fn app_action_adapter_is_send_sync_for_tokio() {
    // Compile-time guarantee: the field type must be Send + Sync so
    // tokio tasks can `tokio::spawn` work that holds an `Arc<dyn
    // ActionAdapter>` (the auto-rejudge / background-sync paths in the
    // App). If anyone makes the trait `?Send`, this fails to compile.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Arc<dyn ActionAdapter>>();
}

#[tokio::test]
async fn action_pane_key_propagates_adapter_errors_to_outcome() {
    // Install a FakeActionAdapter pre-armed to fail on
    // `post_passport_check`, then dispatch the 'A' (Approve) key
    // through the pane. The handler must surface the error as
    // `ActionOutcome::Failed(msg)` in the action pane's last_result so
    // operators see the failure rather than a silent no-op.
    let mut app = app_with_demo_delivery().await;
    let fake = Arc::new(FakeActionAdapter::fail_next("post_passport_check"));
    app.action_adapter = fake.clone();
    app.action_pane.visible = true;

    let consumed = app
        .action_pane_key(crossterm::event::KeyCode::Char('A'))
        .await;
    assert!(consumed);

    match app.action_pane.last_result.as_ref().map(|r| &r.outcome) {
        Some(ActionOutcome::Failed(msg)) => {
            assert!(!msg.is_empty(), "failed outcome must carry a message");
            assert!(
                msg.contains("post_passport_check"),
                "error should name the failing seam: {msg}"
            );
        }
        other => panic!("expected Failed outcome, got {other:?}"),
    }
}
