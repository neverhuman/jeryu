//! Structured error envelope for the JeRyu Web Forge BFF.
//!
//! Implements §35.1.11 of `WEB_WORK_CLAUDE.md`. Every API failure returns
//! a JSON body shaped as:
//!
//! ```json
//! { "error": { "code": "<snake_case>", "message": "...",
//!              "details": { ... }?, "request_id": "...",
//!              "event_cursor": <u64>? } }
//! ```
//!
//! Canonical error codes (lowercase snake_case): unauthenticated, forbidden,
//! csrf_invalid, not_found, bad_request, validation_failed, conflict,
//! merge_sha_stale, settings_hash_stale, idempotency_replay,
//! idempotency_conflict, rate_limited, upstream_unavailable,
//! upstream_forbidden, subscribe_forbidden, event_gap, internal.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorEnvelope,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorEnvelope {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_cursor: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("csrf invalid")]
    CsrfInvalid,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("validation failed: {0}")]
    ValidationFailed(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("merge sha stale: expected {expected} got {live}")]
    MergeShaStale { expected: String, live: String },
    #[error("settings hash stale")]
    SettingsHashStale,
    #[error("idempotency replay")]
    IdempotencyReplay,
    #[error("idempotency conflict")]
    IdempotencyConflict,
    #[error("rate limited")]
    RateLimited,
    #[error("upstream unavailable: {0}")]
    Upstream(String),
    #[error("upstream forbidden: {0}")]
    UpstreamForbidden(String),
    #[error("subscribe forbidden: {0}")]
    SubscribeForbidden(String),
    #[error("event gap: missed seq {0}")]
    EventGap(u64),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden(_) => "forbidden",
            Self::CsrfInvalid => "csrf_invalid",
            Self::NotFound(_) => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::ValidationFailed(_) => "validation_failed",
            Self::Conflict(_) => "conflict",
            Self::MergeShaStale { .. } => "merge_sha_stale",
            Self::SettingsHashStale => "settings_hash_stale",
            Self::IdempotencyReplay => "idempotency_replay",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::RateLimited => "rate_limited",
            Self::Upstream(_) => "upstream_unavailable",
            Self::UpstreamForbidden(_) => "upstream_forbidden",
            Self::SubscribeForbidden(_) => "subscribe_forbidden",
            Self::EventGap(_) => "event_gap",
            Self::Internal(_) => "internal",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) | Self::CsrfInvalid | Self::SubscribeForbidden(_) => {
                StatusCode::FORBIDDEN
            }
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) | Self::ValidationFailed(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_)
            | Self::MergeShaStale { .. }
            | Self::SettingsHashStale
            | Self::IdempotencyConflict
            | Self::EventGap(_) => StatusCode::CONFLICT,
            Self::IdempotencyReplay => StatusCode::OK,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::UpstreamForbidden(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::MergeShaStale { expected, live } => {
                Some(serde_json::json!({"expected": expected, "live": live}))
            }
            Self::EventGap(seq) => Some(serde_json::json!({"missed_seq": seq})),
            _ => None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code();
        let details = self.details();
        let body = ApiErrorBody {
            error: ApiErrorEnvelope {
                code: code.into(),
                message: self.to_string(),
                details,
                // Populated by middleware in W-CC-08 telemetry (request-id layer).
                request_id: None,
                event_cursor: None,
            },
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_match_spec_35_1_11() {
        // Sample the spec'd canonical codes.
        assert_eq!(ApiError::Unauthenticated.code(), "unauthenticated");
        assert_eq!(ApiError::Forbidden("p".into()).code(), "forbidden");
        assert_eq!(ApiError::CsrfInvalid.code(), "csrf_invalid");
        assert_eq!(ApiError::NotFound("x".into()).code(), "not_found");
        assert_eq!(ApiError::BadRequest("x".into()).code(), "bad_request");
        assert_eq!(
            ApiError::ValidationFailed("x".into()).code(),
            "validation_failed"
        );
        assert_eq!(ApiError::Conflict("x".into()).code(), "conflict");
        assert_eq!(
            ApiError::MergeShaStale {
                expected: "a".into(),
                live: "b".into()
            }
            .code(),
            "merge_sha_stale"
        );
        assert_eq!(ApiError::SettingsHashStale.code(), "settings_hash_stale");
        assert_eq!(ApiError::IdempotencyReplay.code(), "idempotency_replay");
        assert_eq!(
            ApiError::IdempotencyConflict.code(),
            "idempotency_conflict"
        );
        assert_eq!(ApiError::RateLimited.code(), "rate_limited");
        assert_eq!(
            ApiError::Upstream("x".into()).code(),
            "upstream_unavailable"
        );
        assert_eq!(
            ApiError::UpstreamForbidden("x".into()).code(),
            "upstream_forbidden"
        );
        assert_eq!(
            ApiError::SubscribeForbidden("x".into()).code(),
            "subscribe_forbidden"
        );
        assert_eq!(ApiError::EventGap(7).code(), "event_gap");
        let internal: ApiError = anyhow::anyhow!("boom").into();
        assert_eq!(internal.code(), "internal");
    }

    #[test]
    fn merge_sha_stale_status_is_409() {
        let e = ApiError::MergeShaStale {
            expected: "a".into(),
            live: "b".into(),
        };
        assert_eq!(e.status(), StatusCode::CONFLICT);
        assert!(e.details().is_some());
    }
}
