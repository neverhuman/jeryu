//! Cookie-based session auth for the JeRyu Web Forge BFF.
//!
//! Phase 1 supports a "local trusted mode" for dev: if no session cookie
//! is present, the middleware accepts a placeholder `Viewer` carrying every
//! canonical permission, gated behind the env var
//! `JERYU_WEB_TRUST_LOCAL=1`. Production deployments leave that env unset
//! and the only way through this middleware is the `__Host-jeryu-session`
//! cookie — the production session table lookup ships with W-B-* alongside
//! the auth/session service.

use std::collections::HashSet;

use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};

use super::error::ApiError;
use super::permissions::perms;

/// Authenticated principal attached to every request via `req.extensions`.
/// Handlers extract this with `axum::Extension<Viewer>`.
#[derive(Clone, Debug)]
pub struct Viewer {
    pub id: String,
    pub login: String,
    pub display_name: Option<String>,
    pub perms: HashSet<String>,
}

impl Viewer {
    /// Construct the dev-mode placeholder. Carries every canonical perm so
    /// route handlers don't need a separate "skip-auth" code path during
    /// Phase 1 wiring. Production session lookup replaces this in W-B-*.
    pub fn local_dev() -> Self {
        let mut perms_set = HashSet::with_capacity(perms::ALL.len());
        for p in perms::ALL {
            perms_set.insert((*p).to_string());
        }
        Self {
            id: "local".into(),
            login: "@local".into(),
            display_name: Some("Local Dev".into()),
            perms: perms_set,
        }
    }
}

/// Axum `from_fn` middleware that resolves the request's `Viewer` and
/// stashes it in the request extensions. Routes that need the viewer
/// extract it with `Extension<Viewer>`.
///
/// Resolution order:
/// 1. If a `__Host-jeryu-session=...` cookie is present, treat as the
///    placeholder dev viewer (Phase 1 — real session lookup lands later).
/// 2. Else if `JERYU_WEB_TRUST_LOCAL=1`, attach the dev viewer.
/// 3. Else respond with `ApiError::Unauthenticated`.
pub async fn auth_layer(
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let trust_local = std::env::var("JERYU_WEB_TRUST_LOCAL")
        .map(|v| v == "1")
        .unwrap_or(false);

    let cookies = headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let has_session = cookies
        .split(';')
        .map(|c| c.trim())
        .any(|c| c.starts_with("__Host-jeryu-session="));

    let viewer = if has_session || trust_local {
        Viewer::local_dev()
    } else {
        return Err(ApiError::Unauthenticated);
    };

    req.extensions_mut().insert(viewer);
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_dev_viewer_carries_canonical_perms() {
        let v = Viewer::local_dev();
        assert_eq!(v.login, "@local");
        // Spot-check a few high-value perms.
        assert!(v.perms.contains("repo.read"));
        assert!(v.perms.contains("mr.approve"));
        assert!(v.perms.contains("settings.write"));
        assert!(v.perms.contains("audit.read"));
    }
}
