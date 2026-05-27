//! Router assembly for the JeRyu Web Forge BFF (W-B-02 + W-B-06/07/09/10).
//!
//! Layout:
//! * `/api/v1/*` — JSON REST endpoints (auth + CSRF on mutating verbs).
//! * `/api/v1/ws` — WebSocket upgrade (auth required, no CSRF).
//! * `/health`, `/hooks`, `/cache/summary` — legacy engine routes,
//!   preserved verbatim per §35.1.5 by merging the supplied `legacy`
//!   router as-is.
//! * Fallback — SPA static-asset service from `<spa-dir>` (W-B-03).
//!
//! Middleware order (outermost → innermost):
//! 1. Telemetry (request-id, trace, timeout, compression) — W-CC-08.
//! 2. CSRF — W-CC-09 (mutating routes only; the layer self-skips safe
//!    methods).
//! 3. Auth — W-CC-07 (every `/api/v1/*` route).
//!
//! Auth runs innermost so handlers see the resolved viewer; CSRF runs
//! after auth so it can reject anonymous browsers without false-flagging
//! them as CSRF.

use axum::{
    Router, middleware,
    routing::{get, post},
};

use super::auth::auth_layer;
use super::csrf::csrf_layer;
use super::rest::{bootstrap::get_bootstrap, markdown, repo_browser, repos, settings};
use super::state::WebState;
use super::static_assets::spa_service;
use super::telemetry::instrument;
use super::ws::ws_handler;

/// Build the combined router (legacy engine + Web Forge BFF + SPA).
///
/// Returns a `Router<()>` ready to be passed to `axum::serve`.
pub fn build_web_router(state: WebState, legacy: Router, spa_dir: &str) -> Router {
    // /api/v1/* — REST + WS. Auth + CSRF apply here; the SPA + legacy
    // routes stay outside the auth perimeter.
    let api = Router::new()
        .route("/api/v1/bootstrap", get(get_bootstrap))
        .route("/api/v1/ws", get(ws_handler))
        // ── W-B-06 + W-B-07: repos + settings ──
        .route(
            "/api/v1/repos",
            get(repos::list_repos).post(repos::create_repo),
        )
        .route("/api/v1/repos/preview", post(repos::create_repo_preview))
        .route(
            "/api/v1/repos/{repo_id}",
            get(repos::get_repo).patch(repos::patch_repo),
        )
        .route(
            "/api/v1/repos/{repo_id}/settings",
            get(settings::get_settings).patch(settings::patch_settings),
        )
        .route(
            "/api/v1/repos/{repo_id}/settings/preview",
            post(settings::preview_settings_patch),
        )
        // ── W-B-09 + W-B-10: repo browser ──
        .route(
            "/api/v1/repos/{repo_id}/refs",
            get(repo_browser::list_refs),
        )
        .route(
            "/api/v1/repos/{repo_id}/tree",
            get(repo_browser::get_tree),
        )
        .route(
            "/api/v1/repos/{repo_id}/blob",
            get(repo_browser::get_blob),
        )
        .route(
            "/api/v1/repos/{repo_id}/raw",
            get(repo_browser::get_raw),
        )
        .route(
            "/api/v1/repos/{repo_id}/readme",
            get(repo_browser::get_readme),
        )
        .route(
            "/api/v1/repos/{repo_id}/compare",
            get(repo_browser::compare_refs),
        )
        .route(
            "/api/v1/repos/{repo_id}/commits",
            get(repo_browser::list_commits),
        )
        .route(
            "/api/v1/repos/{repo_id}/history",
            get(repo_browser::list_commits),
        )
        .route(
            "/api/v1/repos/{repo_id}/blame",
            get(repo_browser::get_blame),
        )
        // ── §35.1.8: standalone markdown render ──
        .route(
            "/api/v1/markdown/render",
            post(markdown::render_markdown_handler),
        )
        .layer(middleware::from_fn(csrf_layer))
        .layer(middleware::from_fn(auth_layer))
        .with_state(state);

    let merged = legacy
        .merge(api)
        .fallback_service(spa_service(spa_dir));

    instrument(merged)
}
