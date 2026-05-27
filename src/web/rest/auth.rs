//! Authentication REST endpoints — `POST /api/v1/auth/login` +
//! `POST /api/v1/auth/logout`.
//!
//! v1 supports a single `provider: "local"` flow gated behind the
//! `JERYU_LOCAL_USERS` env var, formatted as
//! `login1:perm.a,perm.b|login2:perm.c`. Any login not listed in the env
//! var is rejected; if the var is unset (or empty), every login attempt
//! fails. GitLab OAuth is a v1.1 work item — `provider: "gitlab"`
//! returns `501 not_implemented`.
//!
//! Successful login issues both the session cookie
//! (`__Host-jeryu-session`) and a fresh CSRF cookie
//! (`__Host-jeryu-csrf`). Logout deletes the session row and clears both
//! cookies with `Max-Age=0`.

use std::collections::HashSet;
use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use jeryu::api::web_read_model::Viewer as DtoViewer;

use crate::web::auth::SESSION_COOKIE_NAME;
use crate::web::csrf::CSRF_COOKIE_NAME;
use crate::web::error::ApiError;
use crate::web::state::WebState;

/// Session TTL — 30 days, matching the cookie `Max-Age`.
pub const SESSION_TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// `JERYU_LOCAL_USERS` env var name — v1 local-provider allowlist.
const LOCAL_USERS_ENV: &str = "JERYU_LOCAL_USERS";

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub provider: String,
    /// Required when `provider == "local"`.
    #[serde(default)]
    pub login: Option<String>,
    /// Reserved for future providers (e.g. GitLab OAuth).
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub viewer: DtoViewer,
}

/// Parse `JERYU_LOCAL_USERS` into `login → permissions` pairs. Format:
/// `login1:perm.a,perm.b|login2:perm.c`. Empty segments and unknown
/// shapes are silently skipped so a stray pipe doesn't kill the whole
/// table.
fn parse_local_users(raw: &str) -> Vec<(String, HashSet<String>)> {
    raw.split('|')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            let (login, perms) = entry.split_once(':')?;
            let login = login.trim();
            if login.is_empty() {
                return None;
            }
            let perms: HashSet<String> = perms
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect();
            Some((login.to_string(), perms))
        })
        .collect()
}

/// Look up a local-user login. `None` means the allowlist is empty or
/// the user is not present. Reads `JERYU_LOCAL_USERS` on every call so a
/// hot reload of the env var (e.g. in `jeryu serve` restarts) is
/// observable without a rebuild.
fn local_user_perms(login: &str) -> Option<HashSet<String>> {
    let raw = std::env::var(LOCAL_USERS_ENV).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    parse_local_users(&raw)
        .into_iter()
        .find(|(l, _)| l == login)
        .map(|(_, p)| p)
}

/// Generate a 16-byte (32 hex char) CSRF token. Distinct from the
/// session id so the CSRF cookie value can be safely echoed into the
/// `X-CSRF-Token` request header by the SPA.
fn new_csrf_token() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Build a `Set-Cookie` header for the session cookie. `__Host-` prefix
/// requires `Secure`, `Path=/`, and no `Domain` attribute.
fn session_cookie_header(id: &str, max_age: u64) -> HeaderValue {
    let value = format!(
        "{}={}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
        SESSION_COOKIE_NAME, id, max_age
    );
    HeaderValue::from_str(&value).expect("session cookie value is ascii")
}

/// Build a `Set-Cookie` header for the CSRF cookie. NOT `HttpOnly` — the
/// SPA needs to read this value to echo into the `X-CSRF-Token` request
/// header for mutating verbs.
fn csrf_cookie_header(token: &str, max_age: u64) -> HeaderValue {
    let value = format!(
        "{}={}; Path=/; Secure; SameSite=Lax; Max-Age={}",
        CSRF_COOKIE_NAME, token, max_age
    );
    HeaderValue::from_str(&value).expect("csrf cookie value is ascii")
}

/// `POST /api/v1/auth/login` — opaque-session login.
///
/// CSRF-exempt (the SPA does not yet have a CSRF cookie). The handler
/// is exposed OUTSIDE the auth middleware so anonymous browsers can
/// reach it.
pub async fn login(
    State(state): State<WebState>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    match req.provider.as_str() {
        "local" => login_local(state, req).await,
        "gitlab" => Ok(not_implemented(
            "gitlab",
            "OAuth login lands in v1.1; v1 supports provider=\"local\" only",
        )),
        other => Err(ApiError::BadRequest(format!(
            "unsupported auth provider {other:?}; expected \"local\" or \"gitlab\""
        ))),
    }
}

/// Build a structured 501 response that matches the §35.1.11 error
/// envelope. `ApiError` lacks a dedicated `NotImplemented` variant
/// because every other endpoint in the BFF either implements the
/// behavior or returns a different 4xx/5xx; the GitLab auth stub is the
/// only place that needs 501 today.
fn not_implemented(provider: &str, msg: &str) -> Response {
    let body = json!({
        "error": {
            "code": "not_implemented",
            "message": format!("auth provider {provider:?}: {msg}"),
        }
    });
    (StatusCode::NOT_IMPLEMENTED, Json(body)).into_response()
}

async fn login_local(state: WebState, req: LoginRequest) -> Result<Response, ApiError> {
    let login = req
        .login
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::BadRequest("missing field: login".into()))?;

    let perms = local_user_perms(login).ok_or_else(|| {
        ApiError::Forbidden(format!("local user {login:?} not in JERYU_LOCAL_USERS"))
    })?;

    let session = state
        .session_store
        .create(
            login, // for local provider, actor_id == login.
            login,
            perms.clone(),
            Duration::from_secs(SESSION_TTL_SECS),
        )
        .await
        .map_err(ApiError::Internal)?;

    let csrf = new_csrf_token();

    let mut perms_vec: Vec<String> = perms.into_iter().collect();
    perms_vec.sort();

    let body = LoginResponse {
        viewer: DtoViewer {
            id: session.actor_id.clone(),
            login: session.actor_login.clone(),
            display_name: None,
            avatar_url: None,
            global_permissions: perms_vec,
        },
    };

    let mut headers = HeaderMap::new();
    headers.append(
        header::SET_COOKIE,
        session_cookie_header(&session.id, SESSION_TTL_SECS),
    );
    headers.append(
        header::SET_COOKIE,
        csrf_cookie_header(&csrf, SESSION_TTL_SECS),
    );

    Ok((StatusCode::OK, headers, Json(body)).into_response())
}

/// `POST /api/v1/auth/logout` — revoke the current session and clear
/// both cookies. Returns 204 No Content even if the session was already
/// gone (idempotent).
pub async fn logout(
    State(state): State<WebState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let cookies = headers
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if let Some(id) = extract_cookie_value(cookies, SESSION_COOKIE_NAME) {
        state
            .session_store
            .revoke(id)
            .await
            .map_err(ApiError::Internal)?;
    }

    let mut out = HeaderMap::new();
    out.append(header::SET_COOKIE, session_cookie_header("", 0));
    out.append(header::SET_COOKIE, csrf_cookie_header("", 0));
    Ok((StatusCode::NO_CONTENT, out).into_response())
}

fn extract_cookie_value<'a>(cookies: &'a str, name: &str) -> Option<&'a str> {
    cookies.split(';').map(str::trim).find_map(|c| {
        let (k, v) = c.split_once('=')?;
        if k == name { Some(v) } else { None }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_local_users_handles_canonical_form() {
        let users = parse_local_users("alice:repo.read,repo.write|bob:repo.read");
        assert_eq!(users.len(), 2);
        let (alice_login, alice_perms) = &users[0];
        assert_eq!(alice_login, "alice");
        assert!(alice_perms.contains("repo.read"));
        assert!(alice_perms.contains("repo.write"));
        let (bob_login, bob_perms) = &users[1];
        assert_eq!(bob_login, "bob");
        assert_eq!(bob_perms.len(), 1);
    }

    #[test]
    fn parse_local_users_skips_empty_and_malformed() {
        let users = parse_local_users("|alice:repo.read|:perm|bob|carol:");
        // alice has perms; bob has no colon (skipped); carol has empty perms (kept).
        let logins: Vec<_> = users.iter().map(|(l, _)| l.clone()).collect();
        assert!(logins.contains(&"alice".to_string()));
        assert!(logins.contains(&"carol".to_string()));
        assert!(!logins.contains(&"bob".to_string()));
    }

    #[test]
    fn parse_local_users_trims_whitespace_around_perm_keys() {
        let users = parse_local_users("alice: repo.read , repo.write ");
        let (_, perms) = &users[0];
        assert!(perms.contains("repo.read"));
        assert!(perms.contains("repo.write"));
    }
}
