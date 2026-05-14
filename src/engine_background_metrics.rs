use axum::{extract::State, http::HeaderMap};
use tracing::warn;

use super::SharedState;

pub(crate) async fn cache_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let token = headers
        .get("X-Jeryu-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token != state.webhook_secret {
        warn!("cache_summary rejected: missing or invalid X-Jeryu-Token");
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    let metrics = match state.db.get_cache_metrics().await {
        Ok(m) => m,
        Err(_) => Default::default(),
    };
    Ok(axum::Json(serde_json::json!({
        "bytes_served": metrics.bytes_served,
        "hits": metrics.hit_count,
        "objects": metrics.object_count,
        "status": "healthy"
    })))
}
