//! Double-submit CSRF middleware (W-CC-09).
//!
//! Mutating requests (POST/PUT/PATCH/DELETE) must present:
//! * a `__Host-jeryu-csrf=<token>` cookie, AND
//! * an `X-CSRF-Token: <token>` header
//!
//! whose values match byte-for-byte. GET/HEAD/OPTIONS are bypassed because
//! they are safe per RFC 9110 §9.2.1. The login route `POST /api/v1/auth/login`
//! is also bypassed because the cookie does not yet exist at that point.
//!
//! Phase 1 generates the CSRF token client-side (the SPA reads the cookie
//! and echoes it into the header); the BFF only validates the equality.
//! A future hardening pass (W-* TBD) signs the token with a server secret.

use axum::{
    extract::Request,
    http::{HeaderMap, Method},
    middleware::Next,
    response::Response,
};

use super::error::ApiError;

pub(crate) const CSRF_COOKIE_NAME: &str = "__Host-jeryu-csrf";
const CSRF_HEADER_NAME: &str = "x-csrf-token";

/// Routes that bypass CSRF even though they accept POST. The login flow
/// happens before the SPA has acquired a CSRF cookie.
const CSRF_BYPASS_PATHS: &[&str] = &["/api/v1/auth/login"];

fn extract_cookie_value<'a>(cookies: &'a str, name: &str) -> Option<&'a str> {
    cookies.split(';').map(str::trim).find_map(|c| {
        let (k, v) = c.split_once('=')?;
        if k == name { Some(v) } else { None }
    })
}

/// Axum `from_fn` middleware enforcing double-submit CSRF on mutating
/// requests. Safe methods and the explicit bypass list short-circuit
/// without inspecting cookies.
pub async fn csrf_layer(
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let is_safe = matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);
    let is_bypass = CSRF_BYPASS_PATHS.iter().any(|p| *p == path);

    if is_safe || is_bypass {
        return Ok(next.run(req).await);
    }

    let cookies = headers
        .get(axum::http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let cookie_token = extract_cookie_value(cookies, CSRF_COOKIE_NAME);
    let header_token = headers
        .get(CSRF_HEADER_NAME)
        .and_then(|h| h.to_str().ok());

    match (cookie_token, header_token) {
        (Some(c), Some(h)) if !c.is_empty() && c == h => Ok(next.run(req).await),
        _ => Err(ApiError::CsrfInvalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_cookie_value_handles_multi_cookie() {
        let cookies = "session=abc; __Host-jeryu-csrf=tok-1; other=foo";
        assert_eq!(
            extract_cookie_value(cookies, "__Host-jeryu-csrf"),
            Some("tok-1")
        );
        assert_eq!(extract_cookie_value(cookies, "session"), Some("abc"));
        assert_eq!(extract_cookie_value(cookies, "missing"), None);
    }

    #[test]
    fn extract_cookie_returns_none_when_absent() {
        assert_eq!(extract_cookie_value("session=x", "__Host-jeryu-csrf"), None);
        assert_eq!(extract_cookie_value("", "__Host-jeryu-csrf"), None);
    }
}
