//! Tower-http glue for the W-CC-08 telemetry middleware.
//!
//! Applies:
//! * `SetRequestIdLayer` (x-request-id, UUID v4)
//! * `PropagateRequestIdLayer` (echo back on the response)
//! * `TraceLayer::new_for_http()` (structured request/response tracing)
//! * `TimeoutLayer` (30 s per-request guard)
//! * `CompressionLayer` (gzip + brotli)
//!
//! The order is deliberate: SetRequestId runs first so subsequent layers
//! see the id; PropagateRequestId runs last to copy it onto the response.

use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

/// Default per-request timeout. The merge/long-running endpoints opt out
/// per-handler when this lands in W-B-* (Phase 1 has no long handlers).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Wrap `router` with the cross-cutting middleware stack. Returns the
/// router for chaining.
pub fn instrument(router: Router) -> Router {
    router.layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            .layer(TraceLayer::new_for_http())
            .layer(TimeoutLayer::with_status_code(
                StatusCode::GATEWAY_TIMEOUT,
                DEFAULT_TIMEOUT,
            ))
            .layer(CompressionLayer::new())
            .layer(PropagateRequestIdLayer::x_request_id()),
    )
}
