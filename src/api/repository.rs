//! Owner: TUI Control-Plane API + Web Forge BFF — repository DTOs
//! Proof: `cargo nextest run -p jeryu --lib api`
//! Invariants: API contract types only; no domain side effects, no I/O.
//!
//! Source: FINAL spec `tips/web/JERYU_FULL_WEB_FORGE_ENGINEERING_SPEC_FINAL.md`
//! §6.2 with §35.1.2 (`RepositoryId` carries opaque `id` plus host/owner/name)
//! and §35.7 (stable id is canonical in API paths).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::entity::{ActionRef, EntityRef, HealthLevel};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum RepositoryHostKind {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "gitlab")]
    GitLab,
    #[serde(rename = "local")]
    Local,
}

/// Opaque, stable repository identifier used on the API surface.
///
/// Per §35.1.2 the canonical key in `/api/v1/repos/{repo_id}` is the opaque
/// `id` (a UUID-shaped string persisted in `web_repositories.id`). The
/// human-readable triple (`host`, `owner`, `name`) is preserved for display
/// in the SPA so we can keep pretty URLs while the backend uses the stable id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct RepositoryId {
    /// Opaque stable identifier (UUID-shaped). Canonical in API paths.
    pub id: String,
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl RepositoryId {
    /// `"{host}/{owner}/{name}"` — display slug for SPA route segments.
    pub fn slug(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.name)
    }

    /// `"{owner}/{name}"` — provider full-name (host context is already on
    /// `host`, so omit it here for GitHub/GitLab parity).
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum RepositoryVisibility {
    Public,
    Internal,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct RepositorySummary {
    pub id: RepositoryId,
    #[ts(type = "{ kind: string; id: string }")]
    #[schemars(with = "EntityRefSurrogate")]
    #[schema(value_type = EntityRefSurrogate)]
    pub entity: EntityRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub visibility: RepositoryVisibility,
    pub default_branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    pub topics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[ts(type = "string")]
    #[schemars(with = "String")]
    #[schema(value_type = String)]
    pub health: HealthLevel,
    pub open_merge_requests: u32,
    pub failing_checks: u32,
    pub running_jobs: u32,
    pub active_agents: u32,
    pub blocked_agents: u32,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clone_http_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clone_ssh_url: Option<String>,
    #[ts(type = "Array<{ action_id: string; label: string; risk: string | null }>")]
    #[schemars(with = "Vec<ActionRefSurrogate>")]
    #[schema(value_type = Vec<ActionRefSurrogate>)]
    pub available_actions: Vec<ActionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct RepositoryListResponse {
    pub generated_at: DateTime<Utc>,
    pub total: u64,
    pub repositories: Vec<RepositorySummary>,
    pub facets: RepositoryFacets,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct RepositoryFacets {
    pub hosts: Vec<String>,
    pub owners: Vec<String>,
    pub families: Vec<String>,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct CreateRepositoryRequest {
    pub host: String,
    pub owner: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub visibility: RepositoryVisibility,
    pub initialize_readme: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitignore_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    pub topics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<RepositoryId>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct CreateRepositoryPreview {
    pub normalized_name: String,
    pub target_owner: String,
    pub visibility: RepositoryVisibility,
    pub initial_files: Vec<String>,
    pub settings_to_apply: Vec<String>,
    pub side_effects: Vec<String>,
    pub warnings: Vec<String>,
}

// ── Schema surrogates for foreign DTO types ─────────────────────────────
//
// `EntityRef`, `HealthLevel`, and `ActionRef` live in `super::entity` and do
// not yet derive `ToSchema`/`JsonSchema`/`TS`. W-F-03 keeps its blast radius
// to `src/api/{new files}` so we declare local surrogate shapes the web-stack
// derives can point at via `schemars(with = ...)` / `schema(value_type = ...)`
// while the field continues to (de)serialize through the real `EntityRef`.
//
// W-F-04 (ts-rs export wiring) and the wider hygiene pass own the question of
// whether to lift these derives onto the upstream types.
#[cfg(feature = "web")]
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
#[allow(dead_code)]
pub(crate) struct EntityRefSurrogate {
    pub kind: String,
    pub id: String,
}

#[cfg(feature = "web")]
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema)]
#[allow(dead_code)]
pub(crate) struct ActionRefSurrogate {
    pub action_id: String,
    pub label: String,
    pub risk: Option<String>,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{EntityKind, EntityRef};

    fn sample_summary() -> RepositorySummary {
        RepositorySummary {
            id: RepositoryId {
                id: "00000000-0000-0000-0000-000000000001".into(),
                host: "gitlab".into(),
                owner: "neverhuman".into(),
                name: "jeryu".into(),
            },
            entity: EntityRef::new(EntityKind::Repo, "neverhuman/jeryu"),
            description: Some("control plane".into()),
            visibility: RepositoryVisibility::Internal,
            default_branch: "main".into(),
            family: Some("veox-*".into()),
            topics: vec!["ci".into(), "agents".into()],
            language: Some("Rust".into()),
            health: HealthLevel::Healthy,
            open_merge_requests: 3,
            failing_checks: 0,
            running_jobs: 1,
            active_agents: 2,
            blocked_agents: 0,
            updated_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            clone_http_url: Some("https://gitlab/neverhuman/jeryu.git".into()),
            clone_ssh_url: None,
            available_actions: vec![],
        }
    }

    #[test]
    fn repository_id_slug_and_full_name() {
        let id = RepositoryId {
            id: "abc".into(),
            host: "gitlab".into(),
            owner: "neverhuman".into(),
            name: "jeryu".into(),
        };
        assert_eq!(id.slug(), "gitlab/neverhuman/jeryu");
        assert_eq!(id.full_name(), "neverhuman/jeryu");
    }

    #[test]
    fn repository_summary_roundtrips() {
        let summary = sample_summary();
        let json = serde_json::to_string(&summary).expect("serialize");
        let back: RepositorySummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id.id, summary.id.id);
        assert_eq!(back.default_branch, "main");
        assert_eq!(back.topics, vec!["ci".to_string(), "agents".into()]);
        assert!(matches!(back.visibility, RepositoryVisibility::Internal));
        assert!(matches!(back.health, HealthLevel::Healthy));
    }

    #[test]
    fn repository_visibility_serializes_snake_case() {
        let v = RepositoryVisibility::Internal;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"internal\"");
    }

    #[test]
    fn host_kind_serializes_snake_case() {
        let v = RepositoryHostKind::GitLab;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"gitlab\"");
    }

    #[test]
    fn create_request_dry_run_default_field() {
        let req = CreateRepositoryRequest {
            host: "gitlab".into(),
            owner: "neverhuman".into(),
            name: "newrepo".into(),
            description: None,
            visibility: RepositoryVisibility::Private,
            initialize_readme: true,
            gitignore_template: None,
            license_template: None,
            default_branch: None,
            topics: vec![],
            family: None,
            template: None,
            dry_run: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateRepositoryRequest = serde_json::from_str(&json).unwrap();
        assert!(back.dry_run);
        assert!(back.initialize_readme);
    }
}
