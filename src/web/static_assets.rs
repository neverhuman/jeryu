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
/// `not_found_service` makes any unmatched request fall back to
/// `index.html` so React Router can render the route client-side. The
/// internal `ServeFile` is wrapped by `tower-http` in a `SetStatus`
/// layer that overrides the 404 with the index body — we surface that
/// concrete type in the return signature so callers (`router.rs`) can
/// reference the exact `ServeDir` flavour without `impl Trait` games.
pub fn spa_service(
    spa_dir: &str,
) -> ServeDir<tower_http::set_status::SetStatus<ServeFile>> {
    let dir = Path::new(spa_dir);
    let index = dir.join("index.html");
    ServeDir::new(spa_dir).not_found_service(ServeFile::new(index))
}
