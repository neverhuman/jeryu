//! Web edge bootstrap, viewer, feature-flag, and live-stream (WebSocket)
//! contracts consumed by the SPA shell and the activity stream.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::repository::RepositorySummary;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Viewer {
    pub id: String,
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    /// Normalized permission keys (24-key set per §35.1.18).
    pub global_permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebFeatureFlags {
    pub repo_create: bool,
    pub settings_write: bool,
    pub merge_write: bool,
    pub markdown_html: bool,
    pub agents: bool,
    pub mcp: bool,
    pub workcells: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebBootstrap {
    pub generated_at: String,
    pub schema_version: String,
    pub viewer: Viewer,
    /// Existing TUI read-model snapshot (`crate::api::read_model::TuiReadModel`).
    /// Serializes inline as a JSON object; the typed schema is opaque from the
    /// web boundary's perspective until W-F-04 lifts derive coverage onto the
    /// TUI read-model.
    #[ts(type = "Record<string, unknown>")]
    pub tui: serde_json::Value,
    pub recent_repositories: Vec<RepositorySummary>,
    pub websocket_url: String,
    pub feature_flags: WebFeatureFlags,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebEvent {
    pub seq: u64,
    pub timestamp: String,
    pub scope: String,
    pub kind: String,
    pub entity: String,
    pub summary: String,
    /// Free-form payload carrying severity, parent, evidence refs, etc.
    #[ts(type = "Record<string, unknown>")]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubscriptionSpec {
    /// Granular topic per §35.1.15, e.g. `global.activity`, `repo.{id}`,
    /// `pr.{id}`, `agent.{id}`, `cache.{id}`. Backend re-checks each scope
    /// against the viewer's perms on every `Subscribe` frame (§35.1.6).
    pub scope: String,
    /// Free-form filter object (e.g. `{ kind: ["pr.approved"] }`).
    #[ts(type = "Record<string, unknown>")]
    pub filters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientWsMessage {
    Hello {
        resume_from: Option<u64>,
        subscriptions: Vec<SubscriptionSpec>,
    },
    Subscribe {
        subscriptions: Vec<SubscriptionSpec>,
    },
    Unsubscribe {
        scopes: Vec<String>,
    },
    Ack {
        seq: u64,
    },
    Ping {
        nonce: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerWsMessage {
    Hello {
        server_time: String,
        current_seq: u64,
        protocol: String,
    },
    SnapshotRequired {
        reason: String,
        current_seq: u64,
    },
    Event {
        event: WebEvent,
    },
    Pong {
        nonce: String,
        server_time: String,
    },
    Error {
        code: String,
        message: String,
    },
}
