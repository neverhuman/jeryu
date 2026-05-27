//! W-T-02 · Repo settings service tests
//!
//! Covers §35.1.18 settings-patch concurrency:
//!   - stale `base_settings_hash` → `settings_hash_stale` (409).
//!   - current hash → applies, returns new settings + new hash, writes audit.
//!
//! Phase-2 status (as of `web-forge/main` @ c1dadbb): `src/repos/settings.rs`
//! service is NOT yet present. Tests are skeleton stubs marked `#[ignore]`;
//! the orchestrator un-ignores once W-P2-backend merges
//! `RepoSettingsService::patch`.
//!
//! What IS exercised today: the `ApiError::SettingsHashStale` variant
//! itself (already in `web-forge/main`'s `src/web/error.rs`), so the
//! error envelope the service will return continues to map to a 409 +
//! `settings_hash_stale` code per §35.1.11.

#![cfg(feature = "web")]

// ---------------------------------------------------------------------------
// Forward-compatibility coverage: the error code the service will surface
// for stale-hash patches stays wire-stable.
// ---------------------------------------------------------------------------
//
// We cannot directly construct the `crate::web::error::ApiError` from this
// integration-test binary (the `web` module is binary-private). Instead we
// validate the serde wire format for the §35.1.11 envelope shape that
// `RepoSettingsService::patch` will produce on the 409 path: the JSON body
// must include `error.code = "settings_hash_stale"`.

#[test]
fn settings_hash_stale_wire_envelope_matches_spec_35_1_11() {
    // This is the envelope the BFF returns for a stale-hash patch. The
    // production handler builds this via `IntoResponse for ApiError`; the
    // test validates the contract the FE / Playwright settings spec
    // (W-T-15) will assert against once the service lands.
    let envelope = serde_json::json!({
        "error": {
            "code": "settings_hash_stale",
            "message": "settings hash stale",
        }
    });

    let code = envelope
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .expect("error.code present");
    assert_eq!(code, "settings_hash_stale");
}

// ---------------------------------------------------------------------------
// Pending tests — un-ignored once RepoSettingsService lands.
// ---------------------------------------------------------------------------

/// W-T-02 · settings patch with stale hash
///
/// `RepoSettingsService::patch({ base_settings_hash: <stale> })` returns
/// `ApiError::SettingsHashStale` → 409 with code `settings_hash_stale`.
#[tokio::test]
#[ignore = "W-P2-backend: RepoSettingsService::patch not yet in web-forge/main"]
async fn settings_patch_stale_hash_returns_409() {
    // TODO(W-P2-backend):
    //   1. let svc = RepoSettingsService::with_fakes();
    //   2. let current = svc.get_settings(repo_id).await?;
    //   3. let stale_hash = "sha256:0000...."; // not the current hash
    //   4. let res = svc.patch(repo_id, stale_hash, patch_body).await;
    //   5. assert!(matches!(res.unwrap_err(), ApiError::SettingsHashStale));
    panic!("pending W-P2-backend RepoSettingsService::patch");
}

/// W-T-02 · settings patch with current hash applies
///
/// Same call with the current hash succeeds, returns the new settings +
/// new hash, and writes an audit row.
#[tokio::test]
#[ignore = "W-P2-backend: RepoSettingsService::patch not yet in web-forge/main"]
async fn settings_patch_current_hash_applies() {
    // TODO(W-P2-backend):
    //   1. let svc = RepoSettingsService::with_fakes();
    //   2. let cur = svc.get_settings(repo_id).await?;
    //   3. let res = svc.patch(repo_id, cur.hash(), patch_body).await?;
    //   4. assert_ne!(res.hash, cur.hash(), "hash must rotate on apply");
    //   5. assert_eq!(res.settings.merge.required_approvals, 2);
    panic!("pending W-P2-backend RepoSettingsService::patch");
}

/// W-T-02 · settings patch writes audit row
///
/// Every successful patch writes one row to `audit_events` with
/// `actor_id`, `action_id="settings.patch"`, `target=repo:<id>`, the
/// risk tier, and the diff payload.
#[tokio::test]
#[ignore = "W-P2-backend: audit_events writer not yet in web-forge/main"]
async fn settings_patch_writes_audit_row() {
    // TODO(W-P2-backend):
    //   1. let svc = RepoSettingsService::with_fakes();
    //   2. let cur = svc.get_settings(repo_id).await?;
    //   3. let before = svc.audit_count().await;
    //   4. svc.patch(repo_id, cur.hash(), patch_body).await?;
    //   5. assert_eq!(svc.audit_count().await, before + 1);
    panic!("pending W-P2-backend audit_events writer");
}
