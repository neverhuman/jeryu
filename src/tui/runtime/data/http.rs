//! Owner: TUI runtime - HttpDataClient (primary `/api/v1/*` transport).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::data::http`
//! Invariants: trailing-slash-normalised base URL; every method attaches
//!             a `context(...)` describing the request so the Source
//!             Doctor (plan §29) has a useful trail; reqwest::Client is
//!             never shared outside this type.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::client::{DataClient, EventPage};
use crate::api::actions::{ActionPreview, ActionResult};
use crate::api::inspection::{ActionRegistryDocument, InspectionEnvelope};
use crate::api::proof::{ProofQuery, ProofTimeline};
use crate::api::read_model::TuiReadModel;
use crate::api::runtime_profile::RuntimeProfile;
use crate::inspection::actions::ExecuteResponse;
use crate::inspection::entity::EntityDetail;

/// Primary `/api/v1/*` transport. Holds a shared `reqwest::Client` so
/// connection pooling is reused; cloning is cheap (Arc inside reqwest).
#[derive(Debug, Clone)]
pub struct HttpDataClient {
    base_url: String,
    client: Client,
}

impl HttpDataClient {
    /// Build a client against `base_url` (e.g. `http://127.0.0.1:18080`).
    /// A trailing slash is trimmed so route paths can lead with `/`.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_client(base_url, Client::new())
    }

    /// Build a client using a caller-provided `reqwest::Client`.
    pub fn with_client(base_url: impl Into<String>, client: Client) -> Self {
        let mut base = base_url.into();
        while base.ends_with('/') {
            base.pop();
        }
        Self {
            base_url: base,
            client,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// GET `path` with optional query params; decode JSON as `T`.
    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<T> {
        let url = self.url(path);
        self.client
            .get(&url)
            .query(params)
            .send()
            .await
            .with_context(|| format!("http: GET {url}"))?
            .error_for_status()
            .with_context(|| format!("http: GET {url} non-2xx"))?
            .json::<T>()
            .await
            .with_context(|| format!("http: decode {url}"))
    }

    /// GET `path` and unwrap the `InspectionEnvelope<T>` so the caller
    /// receives the payload `T` directly. Every inspection read route
    /// returns an envelope (api_version + generated_at + sources +
    /// data); the `HttpDataClient` is the boundary that strips it so
    /// the TUI never sees the wire shape.
    async fn get_envelope<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<T> {
        let envelope: InspectionEnvelope<T> = self.get_json(path, params).await?;
        Ok(envelope.into_data())
    }

    /// POST `path` with a JSON body; decode JSON response as `T`.
    async fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.url(path);
        self.client
            .post(&url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("http: POST {url}"))?
            .error_for_status()
            .with_context(|| format!("http: POST {url} non-2xx"))?
            .json::<T>()
            .await
            .with_context(|| format!("http: decode {url}"))
    }
}

fn proof_params(q: &ProofQuery) -> Result<Vec<(&'static str, String)>> {
    let mut p: Vec<(&'static str, String)> = Vec::new();
    if let Some(k) = q.entity_kind {
        let v = serde_json::to_string(&k)?.trim_matches('"').to_string();
        p.push(("entity_kind", v));
    }
    if let Some(id) = &q.entity_id {
        p.push(("entity_id", id.clone()));
    }
    if let Some(actor) = &q.actor {
        p.push(("actor", actor.clone()));
    }
    if let Some(cursor) = &q.cursor {
        p.push(("cursor", cursor.clone()));
    }
    if let Some(limit) = q.limit {
        p.push(("limit", limit.to_string()));
    }
    if let Some(since) = q.since {
        p.push(("since", since.to_rfc3339()));
    }
    Ok(p)
}

#[async_trait]
impl DataClient for HttpDataClient {
    async fn fetch_read_model(&self) -> Result<TuiReadModel> {
        self.get_envelope("/api/v1/read-model", &[]).await
    }

    async fn fetch_events(&self, cursor: u64, limit: u32) -> Result<EventPage> {
        let limit = limit.min(500);
        let params = [("cursor", cursor.to_string()), ("limit", limit.to_string())];
        self.get_envelope("/api/v1/events", &params).await
    }

    async fn fetch_proof(&self, query: ProofQuery) -> Result<ProofTimeline> {
        let params = proof_params(&query)?;
        self.get_envelope("/api/v1/proof", &params).await
    }

    async fn fetch_entity(&self, kind: &str, id: &str) -> Result<serde_json::Value> {
        // Entity route is the one read endpoint that does not have a
        // fixed Rust type on the client (each kind has its own
        // projection shape). Unwrap the envelope into an `EntityDetail`
        // and re-serialize as `serde_json::Value` so the public
        // `DataClient` surface stays unchanged.
        let detail: EntityDetail = self
            .get_envelope(&format!("/api/v1/entity/{kind}/{id}"), &[])
            .await?;
        serde_json::to_value(detail).with_context(|| "http: serialize entity detail")
    }

    async fn fetch_runtime_profile(&self) -> Result<RuntimeProfile> {
        self.get_envelope("/api/v1/runtime/profile", &[]).await
    }

    async fn fetch_action_registry(&self) -> Result<Vec<serde_json::Value>> {
        let doc: ActionRegistryDocument = self.get_envelope("/api/v1/action-registry", &[]).await?;
        Ok(doc.actions)
    }

    async fn preview_action(
        &self,
        action_id: &str,
        args: serde_json::Value,
    ) -> Result<ActionPreview> {
        let body = serde_json::json!({ "action_id": action_id, "args": args });
        self.post_json("/api/v1/action/preview", &body).await
    }

    async fn execute_action(
        &self,
        action_id: &str,
        args: serde_json::Value,
        idempotency_key: Option<String>,
    ) -> Result<ActionResult> {
        let body = serde_json::json!({
            "action_id": action_id,
            "args": args,
            "idempotency_key": idempotency_key,
        });
        let resp: ExecuteResponse = self.post_json("/api/v1/action/execute", &body).await?;
        Ok(resp.result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::SCHEMA_VERSION;
    use crate::inspection::serve::serve_inspection;
    use crate::inspection::state::InspectionState;
    use tokio::net::TcpListener;

    async fn spawn() -> (String, tokio::task::JoinHandle<Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let state = InspectionState::default();
        let handle = tokio::spawn(async move { serve_inspection(listener, state).await });
        (base, handle)
    }

    #[tokio::test]
    async fn http_client_round_trips_read_model() {
        let (base, handle) = spawn().await;
        let client = HttpDataClient::new(base);
        let model = client.fetch_read_model().await.expect("fetch_read_model");
        assert_eq!(model.schema_version, SCHEMA_VERSION);
        handle.abort();
    }

    #[tokio::test]
    async fn http_client_fetches_runtime_profile() {
        let (base, handle) = spawn().await;
        let client = HttpDataClient::new(base);
        let profile = client.fetch_runtime_profile().await.expect("profile");
        assert_eq!(profile.inspection_api_version, "api.v1");
        handle.abort();
    }

    #[tokio::test]
    async fn http_client_fetches_events_empty_page() {
        let (base, handle) = spawn().await;
        let client = HttpDataClient::new(base);
        let page = client.fetch_events(0, 100).await.expect("events");
        assert!(page.items.is_empty());
        handle.abort();
    }

    #[tokio::test]
    async fn http_client_preview_returns_disabled_stub() {
        let (base, handle) = spawn().await;
        let client = HttpDataClient::new(base);
        let preview = client
            .preview_action("open_logs", serde_json::json!({}))
            .await
            .expect("preview");
        assert!(!preview.enabled);
        handle.abort();
    }

    #[tokio::test]
    async fn http_client_execute_returns_action_result() {
        let (base, handle) = spawn().await;
        let client = HttpDataClient::new(base);
        let result = client
            .execute_action("open_logs", serde_json::json!({}), Some("k-1".into()))
            .await
            .expect("execute");
        assert!(result.summary.contains("open_logs"));
        handle.abort();
    }

    #[tokio::test]
    async fn http_client_fetch_action_registry_unwraps_envelope_to_action_list() {
        let (base, handle) = spawn().await;
        let client = HttpDataClient::new(base);
        let actions = client.fetch_action_registry().await.expect("registry");
        // The DataClient surface still returns `Vec<serde_json::Value>`
        // even though the wire is `InspectionEnvelope<ActionRegistryDocument>`.
        assert!(actions.len() >= 30, "registry too small: {}", actions.len());
        assert!(actions[0].get("id").and_then(|v| v.as_str()).is_some());
        handle.abort();
    }

    #[tokio::test]
    async fn http_client_fetch_entity_unwraps_envelope_to_detail_json() {
        let (base, handle) = spawn().await;
        let client = HttpDataClient::new(base);
        let detail = client.fetch_entity("job", "j-1").await.expect("entity");
        // `fetch_entity` returns `serde_json::Value` re-serialised from
        // the `EntityDetail` unwrapped from the envelope.
        assert!(detail.get("entity").is_some());
        assert_eq!(
            detail.get("label").and_then(|v| v.as_str()),
            Some("job:j-1")
        );
        handle.abort();
    }

    #[test]
    fn base_url_strips_trailing_slash() {
        let c = HttpDataClient::new("http://localhost:1234/");
        assert_eq!(c.base_url(), "http://localhost:1234");
    }
}
