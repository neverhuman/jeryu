//! Owner: Inspection HTTP plane - axum serve wrapper for the read plane.
//! Proof: `cargo test -p jeryu --lib inspection::serve`
//! Invariants: serve_inspection binds the inspection router on the supplied
//!             listener and returns when the listener closes. Tests pass a
//!             127.0.0.1:0 listener to obtain the ephemeral port.

use anyhow::Result;
use tokio::net::TcpListener;

use super::router::build_router;
use super::state::InspectionState;

/// Run the inspection HTTP plane against `listener`. Returns when the
/// listener is closed (or never if it stays open). Tests use this with an
/// ephemeral port (`127.0.0.1:0`).
pub async fn serve_inspection(listener: TcpListener, state: InspectionState) -> Result<()> {
    let app = build_router(state);
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
    use tokio::task;

    /// End-to-end smoke: bind ephemeral port, send a GET, verify response.
    /// Validates the router shape and serve wrapper as one unit.
    async fn spawn_inspection() -> (String, task::JoinHandle<Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let state = InspectionState::default();
        let handle = tokio::spawn(async move { serve_inspection(listener, state).await });
        (base, handle)
    }

    #[tokio::test]
    async fn read_model_route_responds_with_json() {
        let (base, handle) = spawn_inspection().await;
        let body = reqwest::get(format!("{base}/api/v1/read-model"))
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert!(body.get("schema_version").is_some());
        handle.abort();
    }

    #[tokio::test]
    async fn runtime_profile_route_responds_with_json() {
        let (base, handle) = spawn_inspection().await;
        let body = reqwest::get(format!("{base}/api/v1/runtime/profile"))
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(
            body.get("inspection_api_version").and_then(|v| v.as_str()),
            Some("api.v1")
        );
        handle.abort();
    }

    #[tokio::test]
    async fn entity_route_rejects_unknown_kind_with_400() {
        let (base, handle) = spawn_inspection().await;
        let resp = reqwest::get(format!("{base}/api/v1/entity/unicorn/id"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        handle.abort();
    }

    #[tokio::test]
    async fn events_route_returns_empty_page() {
        let (base, handle) = spawn_inspection().await;
        let body = reqwest::get(format!("{base}/api/v1/events"))
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(
            body.get("items")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(0)
        );
        handle.abort();
    }
}
