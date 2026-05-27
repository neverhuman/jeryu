//! SPA static-asset service.
//!
//! Production mode serves the built SPA from `<spa-dir>`:
//!
//! * `/assets/*` is served directly from `<spa-dir>/assets/` by
//!   [`ServeDir`] — bundled JS/CSS/images keep their natural
//!   `Content-Type` + `200`/`404` semantics.
//! * Every other route falls through to [`spa_fallback`], which reads
//!   `index.html` from disk and returns it with an explicit
//!   `HTTP/1.1 200 OK` + `text/html; charset=utf-8`. React Router then
//!   resolves the in-browser path.
//!
//! The earlier shape (`ServeDir::new(spa_dir).fallback(ServeFile::new(index))`)
//! relied on `tower-http`'s internal chain to flip the 404 from the
//! `ServeDir::call_inner` not-found branch into the fallback `ServeFile`
//! response. In practice this was fragile — the chain occasionally
//! propagated the 404 status onto the fallback body (Phase 1 retro audit
//! finding: `GET /repos/github/foo/bar → 404` with the index HTML body).
//! A custom handler is unambiguous: there is exactly one branch that
//! produces a response, and it always sets `StatusCode::OK`.
//!
//! Dev-mode reverse-proxy (`--dev-assets <url>`) is intentionally
//! deferred to v1.1; for Phase 1 developers run `npm --workspace
//! @jeryu/web run dev` separately and point their browser at Vite's port.

use std::path::PathBuf;

use axum::{
    Router,
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use tower_http::services::ServeDir;

/// Build a [`Router`] that serves the SPA from `spa_dir`.
///
/// * `GET /assets/*` → [`ServeDir`] over `<spa_dir>/assets/` (natural
///   `200` for hits, `404` for misses).
/// * Every other path → [`spa_fallback`] → `index.html` with `200 OK`.
///
/// The returned router is intended to be `.merge()`d into the outer
/// router; its catch-all `fallback` swallows every unmatched route and
/// is the last thing the outer router consults.
pub fn spa_router(spa_dir: &str) -> Router {
    let dir = PathBuf::from(spa_dir);
    let assets_dir = dir.join("assets");

    let fallback_dir = dir.clone();
    Router::new()
        .nest_service("/assets", ServeDir::new(assets_dir))
        .fallback(get(move || {
            let dir = fallback_dir.clone();
            async move { spa_fallback(dir).await }
        }))
}

/// Read `<spa_dir>/index.html` and return it as `200 OK text/html`.
///
/// Returns `500` if the file is missing or unreadable — that is a
/// deployment-time misconfiguration (the SPA build did not run / the
/// `--spa-dir` flag points somewhere wrong) rather than a client error,
/// so a `5xx` is the honest signal.
async fn spa_fallback(spa_dir: PathBuf) -> Response {
    let index = spa_dir.join("index.html");
    match tokio::fs::read(&index).await {
        Ok(bytes) => {
            let mut resp = Response::new(Body::from(bytes));
            *resp.status_mut() = StatusCode::OK;
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            );
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            resp
        }
        Err(err) => {
            tracing::error!(
                spa_index = %index.display(),
                error = %err,
                "SPA index.html unreadable — check --spa-dir and that the SPA build ran"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "SPA index.html missing — see server logs",
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::to_bytes;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Helper: build a router pointed at the freshly-built SPA dist if
    /// it exists; tests are skipped otherwise so they remain green in
    /// `cargo test` runs that did not pre-build the SPA.
    fn spa_dir_or_skip() -> Option<PathBuf> {
        let dist = PathBuf::from("apps/web/dist");
        if dist.join("index.html").is_file() {
            Some(dist)
        } else {
            None
        }
    }

    #[tokio::test]
    async fn fallback_returns_200_for_unmatched_routes() {
        let Some(dist) = spa_dir_or_skip() else {
            eprintln!("apps/web/dist/index.html not present; skipping");
            return;
        };
        let app = spa_router(dist.to_str().unwrap());

        // Deep client route — must return 200 with the index body.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/repos/github/foo/bar")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "deep SPA route must serve index.html with 200"
        );
        let body = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        assert!(
            body.starts_with(b"<!doctype html") || body.starts_with(b"<!DOCTYPE html"),
            "fallback body must be the index.html document"
        );
    }

    #[tokio::test]
    async fn fallback_returns_200_for_root() {
        let Some(dist) = spa_dir_or_skip() else {
            return;
        };
        let app = spa_router(dist.to_str().unwrap());
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn assets_prefix_serves_real_files() {
        let Some(dist) = spa_dir_or_skip() else {
            return;
        };
        // Find the first asset file under apps/web/dist/assets so we
        // can request it without hardcoding the hashed filename.
        let assets_dir = dist.join("assets");
        let Some(entry) = std::fs::read_dir(&assets_dir)
            .ok()
            .and_then(|mut it| it.next())
            .and_then(|e| e.ok())
        else {
            eprintln!("no files under apps/web/dist/assets; skipping");
            return;
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let app = spa_router(dist.to_str().unwrap());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/assets/{name}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "/assets/{} must serve the real bundled file",
            name
        );
    }

    #[tokio::test]
    async fn missing_assets_404() {
        let Some(dist) = spa_dir_or_skip() else {
            return;
        };
        let app = spa_router(dist.to_str().unwrap());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/assets/this-file-does-not-exist.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // ServeDir returns 404 for a missing asset — important so the
        // SPA never accidentally serves index.html as a JS chunk.
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
