//! Bounded request ID selection and response propagation for every HTTP edge.

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");
const MAX_REQUEST_ID_BYTES: usize = 128;

/// Preserve a caller's correlation token only when it has a small, portable
/// shape. Invalid or oversized values are replaced before downstream handlers
/// see them, and the selected request id is returned on every response.
pub(super) async fn propagate(mut request: Request, next: Next) -> Response {
    let request_id = selected_request_id(request.headers().get(&REQUEST_ID));
    request
        .headers_mut()
        .insert(REQUEST_ID.clone(), request_id.clone());

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(REQUEST_ID.clone(), request_id);
    response
}

fn selected_request_id(candidate: Option<&HeaderValue>) -> HeaderValue {
    candidate
        .filter(|value| request_id_is_safe(value))
        .cloned()
        .unwrap_or_else(|| {
            HeaderValue::from_str(&Uuid::new_v4().to_string())
                .expect("a UUID is always a valid header value")
        })
}

fn request_id_is_safe(value: &HeaderValue) -> bool {
    let Ok(value) = value.to_str() else {
        return false;
    };
    !value.is_empty()
        && value.len() <= MAX_REQUEST_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::middleware::from_fn;
    use axum::routing::get;
    use tower::ServiceExt;

    async fn response_for(request_id: Option<&str>) -> Response {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(from_fn(propagate));
        let mut request = Request::builder().uri("/");
        if let Some(request_id) = request_id {
            request = request.header(REQUEST_ID.clone(), request_id);
        }
        app.oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn safe_caller_request_id_round_trips_exactly() {
        let response = response_for(Some("client:deploy-42_a.b")).await;
        assert_eq!(response.headers()[&REQUEST_ID], "client:deploy-42_a.b");
    }

    #[tokio::test]
    async fn absent_request_id_gets_a_valid_uuid() {
        let response = response_for(None).await;
        let selected = response.headers()[&REQUEST_ID].to_str().unwrap();
        assert!(Uuid::parse_str(selected).is_ok());
    }

    #[tokio::test]
    async fn hostile_or_oversized_request_id_is_replaced() {
        for supplied in ["contains spaces", "slash/is/not/portable", &"x".repeat(129)] {
            let response = response_for(Some(supplied)).await;
            let selected = response.headers()[&REQUEST_ID].to_str().unwrap();
            assert_ne!(selected, supplied);
            assert!(Uuid::parse_str(selected).is_ok());
        }
    }
}
