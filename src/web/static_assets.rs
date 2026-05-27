//! SPA static-asset service.
//!
//! Production mode serves the built SPA from `<spa-dir>` and falls back
//! to `index.html` for client-side routing (React Router catches the
//! actual path).
//!
//! Dev-mode reverse-proxy (`--dev-assets <url>`) is intentionally
//! deferred to v1.1; for Phase 1 developers run `npm --workspace
//! @jeryu/web run dev` separately and point their browser at Vite's port.

use std::path::Path;

use tower_http::services::{ServeDir, ServeFile};

/// Build the `ServeDir` service that backs the SPA fallback.
///
/// Uses `.fallback(ServeFile)` — NOT `.not_found_service()` — so the
/// served index.html keeps its natural 200 status (not wrapped to 404 by
/// tower-http's `not_found_service` layer). Without this fix, deep-link
/// client routes like `/repos/github/foo/bar` would still render in the
/// browser but show as 404 to monitoring/proxies/SEO (audit finding from
/// Phase 1 retro).
pub fn spa_service(spa_dir: &str) -> ServeDir<ServeFile> {
    let dir = Path::new(spa_dir);
    let index = dir.join("index.html");
    ServeDir::new(spa_dir).fallback(ServeFile::new(index))
}
