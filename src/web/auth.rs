//! Cookie-based session auth for the JeRyu Web Forge BFF.
//!
//! Resolution order on every `/api/v1/*` request:
//!
//! 1. Read the `__Host-jeryu-session` cookie. If it names a live row in
//!    the `sessions` table, hydrate the `Viewer` from that row.
//! 2. If no cookie AND `JERYU_WEB_TRUST_LOCAL=1`, fall back to the
//!    `Viewer::local_dev()` placeholder. This keeps the developer loop
//!    one curl away from a working `/api/v1/bootstrap` without
//!    standing up a session.
//! 3. Otherwise → `ApiError::Unauthenticated` (HTTP 401).
//!
//! The session lookup also rate-limits `last_seen_at` writes to one
//! update per minute per session (see
//! [`SessionStore::touch`](crate::web::sessions::SessionStore::touch)).

use std::collections::HashSet;

use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};

use super::error::ApiError;
use super::permissions::perms;
use super::state::WebState;

/// Name of the opaque session cookie. `__Host-` prefix enforces
/// `Secure`, `Path=/`, no `Domain` — the browser refuses to send the
/// cookie cross-site even if a misconfigured server tries to broaden
/// the scope.
pub(crate) const SESSION_COOKIE_NAME: &str = "__Host-jeryu-session";

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
    /// the developer loop (`JERYU_WEB_TRUST_LOCAL=1`).
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

/// Extract a single cookie value from a raw `Cookie:` header. Returns
/// `None` if the header is missing the named cookie or the value is
/// empty.
fn extract_cookie_value<'a>(cookies: &'a str, name: &str) -> Option<&'a str> {
    cookies.split(';').map(str::trim).find_map(|c| {
        let (k, v) = c.split_once('=')?;
        if k == name && !v.is_empty() {
            Some(v)
        } else {
            None
        }
    })
}

/// Axum `from_fn_with_state` middleware that resolves the request's
/// `Viewer` and stashes it in the request extensions. Routes that need
/// the viewer extract it with `Extension<Viewer>`.
pub async fn auth_layer(
    State(state): State<WebState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let cookies = headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let session_cookie = extract_cookie_value(cookies, SESSION_COOKIE_NAME);

    let viewer = if let Some(id) = session_cookie {
        match state
            .session_store
            .find(id)
            .await
            .map_err(ApiError::Internal)?
        {
            Some(record) => {
                // Bump last_seen_at, deduped to once-per-minute. Touch
                // failures are non-fatal — the request still succeeds;
                // we just log them so the operator notices a DB issue.
                if let Err(err) = state.session_store.touch(id).await {
                    tracing::warn!(error = %err, "session touch failed");
                }
                Viewer {
                    id: record.actor_id,
                    login: record.actor_login,
                    display_name: None,
                    perms: record.perms,
                }
            }
            None => return Err(ApiError::Unauthenticated),
        }
    } else if trust_local_enabled() {
        Viewer::local_dev()
    } else {
        return Err(ApiError::Unauthenticated);
    };

    req.extensions_mut().insert(viewer);
    Ok(next.run(req).await)
}

fn trust_local_enabled() -> bool {
    std::env::var("JERYU_WEB_TRUST_LOCAL")
        .map(|v| v == "1")
        .unwrap_or(false)
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

    #[test]
    fn extract_cookie_value_handles_multi_cookie() {
        let cookies = "session=abc; __Host-jeryu-session=tok-1; other=foo";
        assert_eq!(
            extract_cookie_value(cookies, "__Host-jeryu-session"),
            Some("tok-1")
        );
        assert_eq!(extract_cookie_value(cookies, "missing"), None);
    }

    #[test]
    fn extract_cookie_value_returns_none_for_empty() {
        // Logout sets `__Host-jeryu-session=; Max-Age=0`, which the
        // middleware must NOT treat as "session token = empty string".
        assert_eq!(
            extract_cookie_value("__Host-jeryu-session=", "__Host-jeryu-session"),
            None
        );
    }
}
