use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use tracing::{debug, warn};

use super::SharedState;

#[path = "engine_webhook_jobs.rs"]
pub(crate) mod jobs_impl;
#[path = "engine_webhook_pipeline.rs"]
pub(crate) mod pipeline_impl;
#[path = "engine_webhook_push.rs"]
pub(crate) mod push_impl;

pub(crate) async fn health() -> &'static str {
    "ok"
}

pub(crate) async fn handle_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, StatusCode> {
    let token = headers
        .get("X-Gitlab-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if token != state.webhook_secret {
        warn!("webhook rejected: invalid token");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let event_type = headers
        .get("X-Gitlab-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    let delivery_uuid = headers
        .get("X-Gitlab-Webhook-UUID")
        .and_then(|v| v.to_str().ok());
    debug!(event_type, delivery_uuid, "received webhook");

    #[cfg(feature = "jansu-broker")]
    {
        dispatch_via_broker(event_type, delivery_uuid, body).await
    }

    #[cfg(not(feature = "jansu-broker"))]
    {
        let _ = state;
        let _ = body;
        warn!("webhook broker feature is disabled; rejecting webhook");
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

/// Dispatch a webhook payload to the appropriate per-event handler. Public
/// within the crate so the jansu consumer loop can invoke the same path as
/// the inline HTTP handler.
pub(crate) async fn dispatch_inline(state: &SharedState, event_type: &str, body: String) {
    match event_type {
        "Job Hook" => {
            if let Err(e) = jobs_impl::handle_job_event_from_body(state, &body).await {
                warn!(error = %e, "failed to handle Job Hook payload");
            }
        }
        "Pipeline Hook" => {
            if let Err(e) =
                pipeline_impl::handle_pipeline_event_from_body(state.clone(), &body).await
            {
                warn!(error = %e, "failed to handle Pipeline Hook payload");
            }
        }
        "Push Hook" => {
            if let Err(e) = push_impl::handle_push_event_from_body(state.clone(), &body).await {
                warn!(error = %e, "failed to handle Push Hook payload");
            }
        }
        "Merge Request Hook" => {
            debug!("merge request event received (logged, not acted on yet)");
        }
        _ => {
            debug!(event_type, "unhandled webhook event type");
        }
    }
}

#[cfg(feature = "jansu-broker")]
async fn dispatch_via_broker(
    event_type: &str,
    delivery_uuid: Option<&str>,
    body: String,
) -> Result<StatusCode, StatusCode> {
    use crate::messaging::{broker_handle, topics};

    let topic = match event_type {
        "Job Hook" => topics::JOBS,
        "Pipeline Hook" => topics::PIPELINES,
        "Push Hook" => topics::PUSHES,
        "Merge Request Hook" => {
            debug!("merge request event received (logged, not acted on yet)");
            return Ok(StatusCode::ACCEPTED);
        }
        _ => {
            debug!(event_type, "unhandled webhook event type");
            return Ok(StatusCode::ACCEPTED);
        }
    };

    let broker = match broker_handle() {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, "broker not ready; rejecting webhook");
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
    };

    let key_bytes = delivery_uuid.map(str::as_bytes);
    if let Err(e) = broker.send(topic, key_bytes, body.as_bytes()).await {
        warn!(error = %e, topic, "broker produce failed");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(StatusCode::ACCEPTED)
}

fn normalize_ref(value: &str) -> String {
    let stripped = match value.strip_prefix("refs/heads/") {
        Some(s) => Some(s),
        None => value.strip_prefix("refs/tags/"),
    };
    match stripped {
        Some(s) => s.to_string(),
        None => value.to_string(),
    }
}
