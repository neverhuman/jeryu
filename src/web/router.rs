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
use super::rest::{
    actions, activity, agents, auth as auth_rest, bootstrap::get_bootstrap, ci, issues, markdown,
    merge_requests, repo_browser, repos, reviews, search, settings,
};
use super::state::WebState;
use super::static_assets::spa_router;
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
        .route("/api/v1/repos/{repo_id}/refs", get(repo_browser::list_refs))
        .route("/api/v1/repos/{repo_id}/tree", get(repo_browser::get_tree))
        .route("/api/v1/repos/{repo_id}/blob", get(repo_browser::get_blob))
        .route("/api/v1/repos/{repo_id}/raw", get(repo_browser::get_raw))
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
        // ── W-B-11/13: merge requests + Merge Passport ──
        .route(
            "/api/v1/repos/{repo_id}/merge-requests",
            get(merge_requests::list_merge_requests).post(merge_requests::create_merge_request),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}",
            get(merge_requests::get_merge_request).patch(merge_requests::patch_merge_request),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/diff",
            get(merge_requests::get_diff),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/checks",
            get(merge_requests::get_checks),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/blockers",
            get(merge_requests::get_blockers),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/approve",
            post(merge_requests::approve_merge_request),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/request-changes",
            post(merge_requests::request_changes),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/merge",
            post(merge_requests::merge_merge_request),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/close",
            post(merge_requests::close_merge_request),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/reopen",
            post(merge_requests::reopen_merge_request),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/rebase",
            post(merge_requests::rebase_merge_request),
        )
        // ── W-B-12: review threads + comments + verdict ──
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/threads",
            get(reviews::list_threads).post(reviews::create_thread),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/threads/{thread_id}",
            axum::routing::patch(reviews::patch_thread),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/comments",
            post(reviews::create_comment),
        )
        .route(
            "/api/v1/repos/{repo_id}/merge-requests/{iid}/reviews",
            post(reviews::submit_review),
        )
        // ── W-B-14: CI / pipelines / jobs / checks ──
        .route("/api/v1/repos/{repo_id}/pipelines", get(ci::list_pipelines))
        .route(
            "/api/v1/repos/{repo_id}/pipelines/{pipeline_id}",
            get(ci::get_pipeline),
        )
        .route(
            "/api/v1/repos/{repo_id}/pipelines/{pipeline_id}/jobs",
            get(ci::list_pipeline_jobs),
        )
        .route(
            "/api/v1/repos/{repo_id}/jobs/{job_id}/log",
            get(ci::get_job_log),
        )
        .route(
            "/api/v1/repos/{repo_id}/jobs/{job_id}/retry",
            post(ci::retry_job),
        )
        .route(
            "/api/v1/repos/{repo_id}/jobs/{job_id}/cancel",
            post(ci::cancel_job),
        )
        .route("/api/v1/repos/{repo_id}/checks", get(ci::list_checks))
        // ── W-B-15: generic action preview/execute ──
        .route("/api/v1/actions/preview", post(actions::preview_action))
        .route("/api/v1/actions/execute", post(actions::execute_action))
        // ── W-B-16: global search ──
        .route("/api/v1/search", get(search::search))
        // ── W-B-17: activity feed ──
        .route("/api/v1/activity", get(activity::list_activity))
        // ── W-B-30: issues stubs (501) ──
        .route(
            "/api/v1/repos/{repo_id}/issues",
            get(issues::list_issues).post(issues::create_issue),
        )
        .route(
            "/api/v1/repos/{repo_id}/issues/{iid}",
            get(issues::get_issue).patch(issues::patch_issue),
        )
        // ── W-B-31: agents read-only ──
        .route(
            "/api/v1/repos/{repo_id}/agents/sessions",
            get(agents::list_sessions),
        )
        .route(
            "/api/v1/repos/{repo_id}/agents/evidence",
            get(agents::list_evidence),
        )
        // ── W-B-auth: logout is INSIDE auth+csrf so it requires a
        // live session and a matching X-CSRF-Token. Login lives in the
        // sibling `auth_open` router below, OUTSIDE both middlewares,
        // because the SPA has no cookies yet on that request.
        .route("/api/v1/auth/logout", post(auth_rest::logout))
        .layer(middleware::from_fn(csrf_layer))
        .layer(middleware::from_fn_with_state(state.clone(), auth_layer))
        .with_state(state.clone());

    // /api/v1/auth/login — public, no middleware. The CSRF bypass list
    // also includes this path (see csrf.rs::CSRF_BYPASS_PATHS) so the
    // SPA's pre-cookie POST can land.
    let auth_open = Router::new()
        .route("/api/v1/auth/login", post(auth_rest::login))
        .with_state(state);

    // SPA router owns the catch-all fallback. Merging (rather than
    // `.fallback_service(spa_service(...))`) keeps the SPA's `/assets/*`
    // ServeDir as a sibling route — that way a missing asset returns a
    // real `404` instead of being swallowed by the SPA fallback into a
    // 200-with-HTML response (which would otherwise mis-train browsers
    // to treat broken JS chunks as HTML).
    let merged = legacy
        .merge(api)
        .merge(auth_open)
        .merge(spa_router(spa_dir));

    instrument(merged)
}
