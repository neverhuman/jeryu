//! Repository identity, listing, and summary contracts for the web/TUI edge.
//!
//! These are read-model DTOs: provider-neutral projections the SPA consumes
//! over the inspection plane. ts-rs is the source of truth for the emitted
//! TypeScript under `contracts/generated/`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::action::AvailableAction;
use super::entity::EntityHandle;

/// Opaque, stable repository identifier used on the API surface.
///
/// Per §35.1.2 the canonical key in `/api/v1/repos/{repo_id}` is the opaque
/// `id` (a UUID-shaped string persisted in `web_repositories.id`). The
/// human-readable triple (`host`, `owner`, `name`) is preserved for display
/// in the SPA so we can keep pretty URLs while the backend uses the stable id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepositoryId {
    /// Opaque stable identifier (UUID-shaped). Canonical in API paths.
    pub id: String,
    pub host: String,
    pub owner: String,
    pub name: String,
}

/// Source-control host family this read model speaks to. Provider-neutral:
/// `jeryu` is the first-party control plane and `local` is an on-disk mirror.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryHostKind {
    Jeryu,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryVisibility {
    Public,
    Internal,
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepositorySummary {
    pub id: RepositoryId,
    #[ts(type = "{ kind: string; id: string }")]
    pub entity: EntityHandle,
    pub description: Option<String>,
    pub visibility: RepositoryVisibility,
    pub default_branch: String,
    pub family: Option<String>,
    pub topics: Vec<String>,
    pub language: Option<String>,
    pub health: String,
    pub open_pull_requests: u32,
    pub failing_checks: u32,
    pub running_jobs: u32,
    pub active_agents: u32,
    pub blocked_agents: u32,
    pub updated_at: String,
    pub clone_http_url: Option<String>,
    pub clone_ssh_url: Option<String>,
    #[ts(type = "Array<{ action_id: string; label: string; risk: string | null }>")]
    pub available_actions: Vec<AvailableAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepositoryFacets {
    pub hosts: Vec<String>,
    pub owners: Vec<String>,
    pub families: Vec<String>,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepositoryListResponse {
    pub generated_at: String,
    pub total: u64,
    pub repositories: Vec<RepositorySummary>,
    pub facets: RepositoryFacets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateRepositoryRequest {
    pub host: String,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub visibility: RepositoryVisibility,
    pub initialize_readme: bool,
    pub gitignore_template: Option<String>,
    pub license_template: Option<String>,
    pub default_branch: Option<String>,
    pub topics: Vec<String>,
    pub family: Option<String>,
    pub template: Option<RepositoryId>,
    pub dry_run: bool,
}

/// Body of `DELETE /api/v1/repos/{repo_id}`.
///
/// `confirm_full_name` must byte-match the repository's `owner/name` exactly
/// (no wildcards, no normalization) or the request is rejected with 422.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeleteRepositoryRequest {
    pub confirm_full_name: String,
    /// Also delete the managed bare git directory (second tier). Defaults to
    /// false: registry-only removal that leaves storage on disk.
    #[serde(default)]
    pub delete_storage: bool,
}

/// One per-collection removal count inside a [`DeleteRepositoryReceipt`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeletedCount {
    pub collection: String,
    pub removed: u32,
}

/// Receipt returned by a successful `DELETE /api/v1/repos/{repo_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeleteRepositoryReceipt {
    pub repo: RepositoryId,
    pub registry_deleted: bool,
    pub deleted_counts: Vec<DeletedCount>,
    /// True only when `delete_storage` was requested AND the managed bare
    /// directory passed every safety check and was removed.
    pub storage_deleted: bool,
    pub storage_path: Option<String>,
    /// Id of the audit-trail entry recording this deletion.
    pub audit_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateRepositoryPreview {
    pub normalized_name: String,
    pub target_owner: String,
    pub visibility: RepositoryVisibility,
    pub initial_files: Vec<String>,
    pub settings_to_apply: Vec<String>,
    pub side_effects: Vec<String>,
    pub warnings: Vec<String>,
}
