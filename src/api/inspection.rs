//! Owner: TUI Control-Plane API - inspection read contracts
//! Proof: `cargo test -p jeryu --lib api::inspection`
//! Invariants: inspection responses are typed, versioned, and read-only.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::entity::{EntityDetail, HealthLevel};
use super::events::TuiEvent;
use super::freshness::SourceFreshness;
use super::read_model::{ComponentHealth, TuiReadModel};
use super::runtime_profile::RuntimeProfile;

pub const INSPECTION_API_VERSION: &str = "api.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionEnvelope<T> {
    pub api_version: String,
    pub generated_at: DateTime<Utc>,
    pub data: T,
    pub sources: Vec<SourceFreshness>,
}

impl<T> InspectionEnvelope<T> {
    pub fn new(data: T, sources: Vec<SourceFreshness>, generated_at: DateTime<Utc>) -> Self {
        Self {
            api_version: INSPECTION_API_VERSION.into(),
            generated_at,
            data,
            sources,
        }
    }

    pub fn into_data(self) -> T {
        self.data
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPage {
    pub cursor: u64,
    pub next_cursor: u64,
    pub events: Vec<TuiEvent>,
}

impl EventPage {
    pub fn empty(cursor: u64) -> Self {
        Self {
            cursor,
            next_cursor: cursor,
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofDetail {
    pub proof_id: String,
    pub status: String,
    pub summary: String,
    pub entity: Option<EntityDetail>,
    pub evidence_refs: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

impl ProofDetail {
    pub fn unavailable(proof_id: impl Into<String>, generated_at: DateTime<Utc>) -> Self {
        Self {
            proof_id: proof_id.into(),
            status: "unknown".into(),
            summary: "No proof projection is wired for this id yet.".into(),
            entity: None,
            evidence_refs: Vec::new(),
            generated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepHealth {
    pub status: HealthLevel,
    pub components: Vec<ComponentHealth>,
    pub sources: Vec<SourceFreshness>,
    pub checked_at: DateTime<Utc>,
}

impl DeepHealth {
    pub fn from_read_model(model: &TuiReadModel, sources: Vec<SourceFreshness>) -> Self {
        let components = model.system.components().into_iter().cloned().collect();
        let status = sources
            .iter()
            .find(|source| source.state.blocks_risky_action())
            .map_or(model.mission.overall, |_| HealthLevel::Degraded);

        Self {
            status,
            components,
            sources,
            checked_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRegistryDocument {
    pub api_version: String,
    pub action_count: usize,
    pub actions: Vec<serde_json::Value>,
}

impl ActionRegistryDocument {
    pub fn new(actions: Vec<serde_json::Value>) -> Self {
        Self {
            api_version: INSPECTION_API_VERSION.into(),
            action_count: actions.len(),
            actions,
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum InspectionPayload {
    ReadModel(TuiReadModel),
    Events(EventPage),
    Entity(EntityDetail),
    Proof(ProofDetail),
    Runtime(RuntimeProfile),
    DeepHealth(DeepHealth),
    ActionRegistry(ActionRegistryDocument),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::freshness::{SourceFreshness, SourceKind};

    #[test]
    fn envelope_pins_api_version() {
        let now = Utc::now();
        let envelope = InspectionEnvelope::new(
            EventPage::empty(9),
            vec![SourceFreshness::live(SourceKind::InspectionHttp, now, "9")],
            now,
        );

        assert_eq!(envelope.api_version, INSPECTION_API_VERSION);
        assert_eq!(envelope.data.next_cursor, 9);
    }

    #[test]
    fn action_registry_document_counts_actions() {
        let doc = ActionRegistryDocument::new(vec![serde_json::json!({"id": "open_logs"})]);
        assert_eq!(doc.action_count, 1);
    }
}
