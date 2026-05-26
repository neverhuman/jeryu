//! Owner: Interactive TUI subsystem - Flight Deck data clients
//! Proof: `cargo check -p jeryu --message-format=json`
//! Invariants: Transport selection is hidden behind typed client boundaries.

use chrono::Utc;
use serde::de::DeserializeOwned;

use crate::api::actions::ActionStreamPage;
use crate::api::events::TuiEvent;
use crate::api::freshness::{SourceFreshness, SourceKind};
use crate::api::inspection::{EventPage, InspectionEnvelope};
use crate::api::read_model::TuiReadModel;
use crate::api::runtime_profile::RuntimeProfile;
use crate::tui::testing::{FixtureScenario, ScenarioFixture};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataTransport {
    Http,
    McpResource,
    Local,
    Fixture,
}

impl DataTransport {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::McpResource => "mcp",
            Self::Local => "local",
            Self::Fixture => "fixture",
        }
    }
}

pub type DataClientResult<T> = Result<T, DataClientError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataClientError {
    Http(String),
    Decode(String),
    Unsupported(&'static str),
}

impl std::fmt::Display for DataClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(message) => write!(f, "http data client error: {message}"),
            Self::Decode(message) => write!(f, "data decode error: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported data client operation: {message}"),
        }
    }
}

impl std::error::Error for DataClientError {}

#[derive(Clone)]
pub enum FlightDeckDataClient {
    Http(HttpDataClient),
    Fixture(FixtureDataClient),
}

impl FlightDeckDataClient {
    pub fn transport(&self) -> DataTransport {
        match self {
            Self::Http(_) => DataTransport::Http,
            Self::Fixture(_) => DataTransport::Fixture,
        }
    }

    pub async fn read_model(&self) -> DataClientResult<InspectionEnvelope<TuiReadModel>> {
        match self {
            Self::Http(client) => client.get_json("/api/v1/read-model").await,
            Self::Fixture(client) => Ok(client.read_model()),
        }
    }

    pub async fn events(&self, cursor: u64) -> DataClientResult<InspectionEnvelope<EventPage>> {
        match self {
            Self::Http(client) => {
                client
                    .get_json(&format!("/api/v1/events?cursor={cursor}"))
                    .await
            }
            Self::Fixture(client) => Ok(client.events(cursor)),
        }
    }

    pub async fn runtime(&self) -> DataClientResult<InspectionEnvelope<RuntimeProfile>> {
        match self {
            Self::Http(client) => client.get_json("/api/v1/runtime").await,
            Self::Fixture(client) => Ok(client.runtime()),
        }
    }

    pub async fn action_stream(&self, cursor: u64) -> DataClientResult<ActionStreamPage> {
        match self {
            Self::Http(client) => {
                client
                    .get_json(&format!("/api/v1/action-stream?cursor={cursor}"))
                    .await
            }
            Self::Fixture(client) => Ok(client.action_stream(cursor)),
        }
    }
}

#[derive(Clone)]
pub struct HttpDataClient {
    base_url: String,
    client: reqwest::Client,
}

impl HttpDataClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    pub(crate) fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> DataClientResult<T> {
        let response = self
            .client
            .get(self.endpoint(path))
            .send()
            .await
            .map_err(|err| DataClientError::Http(err.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(DataClientError::Http(format!("status {status}")));
        }
        response
            .json::<T>()
            .await
            .map_err(|err| DataClientError::Decode(err.to_string()))
    }
}

#[derive(Clone)]
pub struct FixtureDataClient {
    read_model: TuiReadModel,
    runtime: RuntimeProfile,
    sources: Vec<SourceFreshness>,
    generated_at: chrono::DateTime<Utc>,
    events: Vec<TuiEvent>,
    action_stream: ActionStreamPage,
}

impl FixtureDataClient {
    pub fn healthy() -> Self {
        Self::scenario(FixtureScenario::Healthy)
    }

    pub fn scenario(scenario: FixtureScenario) -> Self {
        let fixture = ScenarioFixture::build(scenario);
        Self {
            read_model: fixture.read_model,
            runtime: fixture.runtime,
            sources: fixture.sources,
            generated_at: fixture.generated_at,
            events: fixture.events,
            action_stream: fixture.action_stream,
        }
    }

    pub fn read_model(&self) -> InspectionEnvelope<TuiReadModel> {
        envelope(
            self.read_model.clone(),
            self.sources.clone(),
            self.generated_at,
        )
    }

    pub fn events(&self, cursor: u64) -> InspectionEnvelope<EventPage> {
        let events: Vec<TuiEvent> = self
            .events
            .iter()
            .filter(|event| event.seq > cursor)
            .cloned()
            .collect();
        let next_cursor = events.last().map_or(cursor, |event| event.seq);
        envelope(
            EventPage {
                cursor,
                next_cursor,
                events,
            },
            self.sources.clone(),
            self.generated_at,
        )
    }

    pub fn runtime(&self) -> InspectionEnvelope<RuntimeProfile> {
        envelope(
            self.runtime.clone(),
            self.sources.clone(),
            self.generated_at,
        )
    }

    pub fn action_stream(&self, cursor: u64) -> ActionStreamPage {
        if self.action_stream.next_cursor > cursor {
            self.action_stream.clone()
        } else {
            ActionStreamPage::empty(cursor)
        }
    }
}

fn envelope<T>(
    data: T,
    sources: Vec<SourceFreshness>,
    generated_at: chrono::DateTime<Utc>,
) -> InspectionEnvelope<T> {
    InspectionEnvelope::new(data, sources, generated_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::SCHEMA_VERSION;

    #[test]
    fn http_endpoint_joins_base_and_path() {
        let client = HttpDataClient::new("http://127.0.0.1:9090/");
        assert_eq!(
            client.endpoint("/api/v1/read-model"),
            "http://127.0.0.1:9090/api/v1/read-model"
        );
    }

    #[tokio::test]
    async fn fixture_client_returns_typed_read_model() {
        let client = FlightDeckDataClient::Fixture(FixtureDataClient::healthy());
        let envelope = client.read_model().await.unwrap();
        assert_eq!(client.transport(), DataTransport::Fixture);
        assert_eq!(envelope.data.schema_version, SCHEMA_VERSION);
        assert_eq!(envelope.sources[0].source, SourceKind::Fixture);
    }

    #[tokio::test]
    async fn fixture_events_preserve_resume_cursor() {
        let client = FlightDeckDataClient::Fixture(FixtureDataClient::healthy());
        let envelope = client.events(42).await.unwrap();
        assert_eq!(envelope.data.cursor, 42);
        assert_eq!(envelope.data.next_cursor, 42);
        assert!(envelope.data.events.is_empty());
    }

    #[tokio::test]
    async fn fixture_scenarios_are_deterministic_and_nonempty() {
        for scenario in FixtureScenario::ALL {
            let client = FlightDeckDataClient::Fixture(FixtureDataClient::scenario(*scenario));
            let read_model = client.read_model().await.unwrap();
            let events = client.events(0).await.unwrap();
            assert_eq!(read_model.generated_at, events.generated_at);
            assert_eq!(
                events.data.events.len(),
                1,
                "missing event for {scenario:?}"
            );
            assert_eq!(events.data.next_cursor, 1);
        }
    }
}
