//! Credential-bound review challenges and complete persisted review history.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChallengeRequest {
    expected_head_sha: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DismissRequest {
    expected_head_sha: Option<String>,
    challenge_id: Option<Uuid>,
    nonce: Option<String>,
    review_id: Uuid,
    reason: String,
}

pub(in crate::web) async fn challenge(
    State(state): State<Arc<WebState>>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    blocking_response("prepare review", move || {
        challenge_blocking(state, id, number, headers, body)
    })
    .await
}

fn challenge_blocking(
    state: Arc<WebState>,
    id: String,
    number: u64,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    let actor = match crate::web::auth::authenticated_actor(&state, &headers) {
        Ok(actor) => actor,
        Err(error) => return core_error(error, "authenticate review holder"),
    };
    let request: ChallengeRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => return invalid_request("prepare review", error),
    };
    let Some(expected_head_sha) = request.expected_head_sha else {
        return missing_head("prepare review");
    };
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("prepare review", "pull request not found");
    };
    match state
        .core
        .create_review_challenge(&actor, repo.id, pr.number, &expected_head_sha)
    {
        Ok(challenge) => (StatusCode::CREATED, Json(challenge)).into_response(),
        Err(error) => core_error(error, "prepare review"),
    }
}

pub(in crate::web) async fn history(
    State(state): State<Arc<WebState>>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
    headers: HeaderMap,
) -> AxumResponse {
    blocking_response("read review history", move || {
        history_blocking(state, id, number, headers)
    })
    .await
}

fn history_blocking(
    state: Arc<WebState>,
    id: String,
    number: u64,
    headers: HeaderMap,
) -> AxumResponse {
    let actor = match crate::web::auth::authenticated_read_actor(&state, &headers) {
        Ok(actor) => actor,
        Err(error) => return core_error(error, "authenticate review history reader"),
    };
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("read review history", "pull request not found");
    };
    match state.core.bound_review_history(&actor, repo.id, pr.number) {
        Ok(history) => Json(history).into_response(),
        Err(error) => core_error(error, "read review history"),
    }
}

pub(in crate::web) async fn dismiss(
    State(state): State<Arc<WebState>>,
    AxumPath((id, number)): AxumPath<(String, u64)>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    blocking_response("dismiss review", move || {
        dismiss_blocking(state, id, number, headers, body)
    })
    .await
}

fn dismiss_blocking(
    state: Arc<WebState>,
    id: String,
    number: u64,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    let actor = match crate::web::auth::authenticated_actor(&state, &headers) {
        Ok(actor) => actor,
        Err(error) => return core_error(error, "authenticate review holder"),
    };
    let request: DismissRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => return invalid_request("dismiss review", error),
    };
    let (Some(challenge_id), Some(nonce)) = (request.challenge_id, request.nonce) else {
        return missing_challenge("dismiss review");
    };
    let Some(expected_head_sha) = request.expected_head_sha else {
        return missing_head("dismiss review");
    };
    let Some((repo, pr)) = resolve_pr(&state, &id, number) else {
        return not_found("dismiss review", "pull request not found");
    };
    match state.core.dismiss_bound_review(
        &actor,
        repo.id,
        pr.number,
        jeryu_core::DismissBoundReviewRequest {
            challenge_id,
            nonce,
            expected_head_sha,
            review_id: request.review_id,
            reason: request.reason,
        },
    ) {
        Ok(event) => Json(event).into_response(),
        Err(error) => core_error(error, "dismiss review"),
    }
}

pub(super) fn missing_challenge(purpose: &'static str) -> AxumResponse {
    repair_error(
        StatusCode::PRECONDITION_REQUIRED,
        "review_challenge_required",
        purpose,
        "a current review challenge and its nonce are required",
        &[
            "POST expected_head_sha to this pull request's review-challenges endpoint",
            "inspect the returned source and evidence, then submit its challenge_id and nonce",
        ],
        PROOF_LANE,
        None,
    )
}

pub(super) fn missing_head(purpose: &'static str) -> AxumResponse {
    core_error(
        ForgeError::PreconditionRequired("expected_head_sha is required".into()),
        purpose,
    )
}

fn invalid_request(purpose: &'static str, error: serde_json::Error) -> AxumResponse {
    core_error(ForgeError::Validation(error.to_string()), purpose)
}

pub(super) async fn blocking_response(
    purpose: &'static str,
    operation: impl FnOnce() -> AxumResponse + Send + 'static,
) -> AxumResponse {
    match tokio::task::spawn_blocking(operation).await {
        Ok(response) => response,
        Err(_) => core_error(
            ForgeError::WriterUnavailable(
                "review operation worker is unavailable; read history before retrying".into(),
            ),
            purpose,
        ),
    }
}

#[cfg(test)]
pub(in crate::web) mod tests;

/// The previous name-only withdrawal URL cannot accept a trusted review.
pub(in crate::web) async fn legacy_dismissal(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
) -> AxumResponse {
    blocking_response(
        "dismiss review",
        move || match crate::web::auth::authenticated_actor(&state, &headers) {
            Ok(_) => missing_challenge("dismiss review through reviews/dismiss"),
            Err(error) => core_error(error, "authenticate review holder"),
        },
    )
    .await
}
