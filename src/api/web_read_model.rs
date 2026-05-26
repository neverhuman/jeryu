//! Owner: TUI Control-Plane API + Web Forge BFF — bootstrap & viewer DTOs
//! Proof: `cargo nextest run -p jeryu --lib api`
//! Invariants: API contract types only; no I/O, no domain side effects.
//!
//! Source: FINAL spec §6.2.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::repository::RepositorySummary;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct WebBootstrap {
    pub generated_at: DateTime<Utc>,
    pub schema_version: String,
    pub viewer: Viewer,
    /// Existing TUI read-model snapshot (`crate::api::read_model::TuiReadModel`).
    /// Serializes inline as a JSON object; the typed schema is opaque from the
    /// web boundary's perspective until W-F-04 lifts derive coverage onto the
    /// TUI read-model.
    #[cfg_attr(feature = "web", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature = "web", schemars(with = "serde_json::Value"))]
    #[cfg_attr(feature = "web", schema(value_type = serde_json::Value))]
    pub tui: super::read_model::TuiReadModel,
    pub recent_repositories: Vec<RepositorySummary>,
    pub websocket_url: String,
    pub feature_flags: WebFeatureFlags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct Viewer {
    pub id: String,
    pub login: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Normalized permission keys (24-key set per §35.1.18).
    pub global_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct WebFeatureFlags {
    pub repo_create: bool,
    pub settings_write: bool,
    pub merge_write: bool,
    pub markdown_html: bool,
    pub agents: bool,
    pub mcp: bool,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;

    fn sample() -> WebBootstrap {
        WebBootstrap {
            generated_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            schema_version: "web.v1.0".into(),
            viewer: Viewer {
                id: "u-1".into(),
                login: "claude".into(),
                display_name: Some("Claude Opus".into()),
                avatar_url: None,
                global_permissions: vec!["repo.read".into(), "mr.read".into()],
            },
            tui: TuiReadModel::default(),
            recent_repositories: vec![],
            websocket_url: "/api/v1/ws".into(),
            feature_flags: WebFeatureFlags {
                repo_create: true,
                settings_write: false,
                merge_write: true,
                markdown_html: true,
                agents: false,
                mcp: false,
            },
        }
    }

    #[test]
    fn web_bootstrap_roundtrip() {
        let b = sample();
        let json = serde_json::to_string(&b).unwrap();
        let back: WebBootstrap = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, "web.v1.0");
        assert_eq!(back.viewer.login, "claude");
        assert_eq!(back.websocket_url, "/api/v1/ws");
        assert!(back.feature_flags.repo_create);
        assert!(!back.feature_flags.settings_write);
    }

    #[test]
    fn viewer_omits_optional_fields_when_absent() {
        let v = Viewer {
            id: "u-1".into(),
            login: "x".into(),
            display_name: None,
            avatar_url: None,
            global_permissions: vec![],
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(!json.contains("display_name"));
        assert!(!json.contains("avatar_url"));
    }
}
