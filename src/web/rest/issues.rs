//! REST handlers for issues (W-B-30 — v1.5 stubs).
//!
//! Issues are scheduled for v1.5 (`WEB_WORK_CLAUDE.md` §35.7 / W-B-30 says
//! "stub returns 501"). The routes are registered so the FE can typecheck
//! against them and we return a structured `not_implemented` envelope so
//! the SPA's error handler renders a graceful banner.

use axum::{Json, extract::Path, http::StatusCode, response::IntoResponse};

use crate::web::error::{ApiErrorBody, ApiErrorEnvelope};

const NOT_IMPLEMENTED_MESSAGE: &str = "Issues are v1.5 — see ROADMAP";

fn not_implemented_body() -> Json<ApiErrorBody> {
    Json(ApiErrorBody {
        error: ApiErrorEnvelope {
            code: "not_implemented".into(),
            message: NOT_IMPLEMENTED_MESSAGE.into(),
            details: None,
            request_id: None,
            event_cursor: None,
        },
    })
}

pub async fn list_issues(Path(_repo_id): Path<String>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, not_implemented_body())
}

pub async fn create_issue(Path(_repo_id): Path<String>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, not_implemented_body())
}

pub async fn get_issue(Path((_repo_id, _iid)): Path<(String, String)>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, not_implemented_body())
}

pub async fn patch_issue(Path((_repo_id, _iid)): Path<(String, String)>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, not_implemented_body())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_carries_canonical_not_implemented_envelope() {
        let body = not_implemented_body();
        assert_eq!(body.0.error.code, "not_implemented");
        assert_eq!(body.0.error.message, NOT_IMPLEMENTED_MESSAGE);
    }
}
