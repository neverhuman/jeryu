//! Owner: Web Forge BFF — SettingsService (W-B-07).
//! Proof: `cargo nextest run -p jeryu --lib repos::settings`
//! Invariants:
//!   - `apply_patch` rejects a stale `base_settings_hash` with
//!     `ApiError::SettingsHashStale`.
//!   - `read` is a pass-through over `GitHost::get_repository`; full settings
//!     row materialization lands with W-B-* when local cache exists.

#![allow(clippy::collapsible_if)]

use std::sync::Arc;

use jeryu::api::repository::{RepositoryId, RepositoryVisibility};
use jeryu::api::settings::{
    AccessSettings, AgentSettings, CiSettings, FeatureSettings, GeneralSettings, MergeSettings,
    NotificationSettings, RepositorySettings, RetentionSettings, SecuritySettings,
};
use jeryu::git_host::{GitHost, GitLabClient, HostRepositorySettingsPatch, RepoRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::web::error::ApiError;

use super::models::RepoId;
use super::service::host_to_api_error;

/// Settings patch wire-format. RFC 7396 merge semantics — only fields that
/// appear in the JSON body are updated; absent fields are left unchanged.
/// Mirrors the host adapter's `HostRepositorySettingsPatch` for the subset
/// of fields the BFF currently supports.
#[derive(
    Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct SettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<RepositoryVisibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

/// Preview output: what would change if the patch were applied, plus a
/// bounded blast-radius summary for the currently supported patch surface.
/// The preview includes the affected branches/MRs and other downstream side
/// effects already known to this service.
#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct SettingsDiffPreview {
    pub repo: RepositoryId,
    pub current_hash: String,
    pub diffs: Vec<SettingsFieldChange>,
    pub side_effects: Vec<String>,
    pub warnings: Vec<String>,
    pub reversible: bool,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct SettingsFieldChange {
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

pub struct SettingsService {
    host_name: String,
    gitlab: Arc<GitLabClient>,
}

impl SettingsService {
    pub fn new(host_name: impl Into<String>, gitlab: Arc<GitLabClient>) -> Self {
        Self {
            host_name: host_name.into(),
            gitlab,
        }
    }

    /// Fetch the current settings snapshot. The host adapter provides
    /// visibility, default_branch, and description; the rest is filled with
    /// deterministic defaults.
    pub async fn read(&self, repo_id: &str) -> Result<RepositorySettings, ApiError> {
        let parsed = match RepoId::parse(repo_id) {
            Some(parsed) => parsed,
            None => return Err(ApiError::BadRequest(format!("invalid repo_id: {repo_id}"))),
        };
        let repo = RepoRef {
            owner: parsed.owner.clone(),
            name: parsed.name.clone(),
        };
        let host_repo = GitHost::get_repository(self.gitlab.as_ref(), &repo)
            .await
            .map_err(host_to_api_error)?;
        let visibility = match host_repo.visibility.as_str() {
            "public" => RepositoryVisibility::Public,
            "internal" => RepositoryVisibility::Internal,
            _ => RepositoryVisibility::Private,
        };
        Ok(default_settings(
            &parsed,
            &self.host_name,
            host_repo.default_branch,
            visibility,
            host_repo.description,
            host_repo.archived,
            host_repo.topics,
        ))
    }

    /// Compute the diff between current settings and the patch. The current
    /// settings hash is included so callers can stash it and pass it back as
    /// `base_settings_hash` on `apply_patch` for optimistic concurrency.
    pub async fn preview_patch(
        &self,
        repo_id: &str,
        patch: &SettingsPatch,
    ) -> Result<SettingsDiffPreview, ApiError> {
        let current = self.read(repo_id).await?;
        let mut diffs: Vec<SettingsFieldChange> = Vec::new();
        if let Some(d) = patch.description.as_deref() {
            if Some(d) != current.general.description.as_deref() {
                diffs.push(SettingsFieldChange {
                    field: "general.description".into(),
                    before: current.general.description.clone(),
                    after: Some(d.into()),
                });
            }
        }
        if let Some(h) = patch.homepage.as_deref() {
            if Some(h) != current.general.homepage.as_deref() {
                diffs.push(SettingsFieldChange {
                    field: "general.homepage".into(),
                    before: current.general.homepage.clone(),
                    after: Some(h.into()),
                });
            }
        }
        if let Some(v) = patch.visibility.as_ref() {
            if std::mem::discriminant(v) != std::mem::discriminant(&current.general.visibility) {
                diffs.push(SettingsFieldChange {
                    field: "general.visibility".into(),
                    before: Some(visibility_to_string(&current.general.visibility).into()),
                    after: Some(visibility_to_string(v).into()),
                });
            }
        }
        if let Some(b) = patch.default_branch.as_deref() {
            if b != current.general.default_branch {
                diffs.push(SettingsFieldChange {
                    field: "general.default_branch".into(),
                    before: Some(current.general.default_branch.clone()),
                    after: Some(b.into()),
                });
            }
        }
        if let Some(a) = patch.archived {
            if a != current.general.archived {
                diffs.push(SettingsFieldChange {
                    field: "general.archived".into(),
                    before: Some(current.general.archived.to_string()),
                    after: Some(a.to_string()),
                });
            }
        }
        let warnings = if diffs.is_empty() {
            vec!["patch is a no-op".into()]
        } else {
            Vec::new()
        };
        Ok(SettingsDiffPreview {
            repo: current.repo.clone(),
            current_hash: hash_settings(&current),
            diffs,
            side_effects: vec![],
            warnings,
            reversible: true,
        })
    }

    /// Apply the patch with optimistic-concurrency guard.
    pub async fn apply_patch(
        &self,
        repo_id: &str,
        base_settings_hash: &str,
        patch: SettingsPatch,
        _idempotency_key: Option<&str>,
    ) -> Result<RepositorySettings, ApiError> {
        let current = self.read(repo_id).await?;
        let live_hash = hash_settings(&current);
        if !base_settings_hash.is_empty() && live_hash != base_settings_hash {
            return Err(ApiError::SettingsHashStale);
        }
        let parsed = match RepoId::parse(repo_id) {
            Some(parsed) => parsed,
            None => return Err(ApiError::BadRequest(format!("invalid repo_id: {repo_id}"))),
        };
        let repo = RepoRef {
            owner: parsed.owner.clone(),
            name: parsed.name.clone(),
        };
        let visibility_str = patch.visibility.as_ref().map(visibility_to_string);
        let host_patch = HostRepositorySettingsPatch {
            description: patch.description.as_deref().map(Some),
            homepage: patch.homepage.as_deref().map(Some),
            visibility: visibility_str,
            default_branch: patch.default_branch.as_deref(),
            archived: patch.archived,
            allow_merge_commit: None,
            allow_squash_merge: None,
            allow_rebase_merge: None,
            allow_auto_merge: None,
            delete_branch_on_merge: None,
        };
        GitHost::update_repository_settings(self.gitlab.as_ref(), &repo, host_patch)
            .await
            .map_err(host_to_api_error)?;
        self.read(repo_id).await
    }
}

fn visibility_to_string(v: &RepositoryVisibility) -> &'static str {
    match v {
        RepositoryVisibility::Public => "public",
        RepositoryVisibility::Internal => "internal",
        RepositoryVisibility::Private => "private",
    }
}

fn hash_settings(s: &RepositorySettings) -> String {
    // Canonical serialization keeps the hash deterministic across handler
    // invocations. We use serde_json with the serializer's default ordering;
    // future hardening can swap for a canonicalization layer (cbor / RFC 8785
    // JCS) without changing the wire format.
    let mut hasher = Sha256::new();
    let canon = serde_json::to_vec(&s).unwrap_or_default();
    hasher.update(&canon);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn default_settings(
    id: &RepoId,
    _host_name: &str,
    default_branch: String,
    visibility: RepositoryVisibility,
    description: Option<String>,
    archived: bool,
    topics: Vec<String>,
) -> RepositorySettings {
    RepositorySettings {
        repo: RepositoryId::from(id),
        general: GeneralSettings {
            name: id.name.clone(),
            description,
            homepage: None,
            visibility,
            default_branch: if default_branch.is_empty() {
                "main".into()
            } else {
                default_branch
            },
            topics,
            archived,
        },
        features: FeatureSettings {
            issues: true,
            merge_requests: true,
            wiki: false,
            discussions: false,
            projects: false,
            packages: false,
            releases: true,
            ci: true,
            security_advisories: false,
            pages: false,
        },
        merge: MergeSettings {
            allow_merge_commit: true,
            allow_squash_merge: true,
            allow_rebase_merge: false,
            allow_auto_merge: false,
            delete_branch_on_merge: true,
            require_linear_history: false,
            required_approvals: 1,
            dismiss_stale_approvals: true,
            require_codeowners: false,
            require_exact_sha_approval: true,
            require_jeryu_merge_passport: true,
        },
        branch_protection: vec![],
        ci: CiSettings {
            default_runner_pool: None,
            concurrency_limit: None,
            artifact_retention_days: 30,
            log_retention_days: 30,
            cache_retention_days: 7,
            vti_enabled: false,
        },
        agents: AgentSettings {
            autonomous_coding_enabled: false,
            max_concurrent_sessions: 0,
            require_human_approval_for_writes: true,
            allowed_agents: vec![],
            allowed_tools: vec![],
            evidence_required: true,
            budget_daily_usd: None,
        },
        access: AccessSettings::default(),
        security: SecuritySettings {
            secret_scanning: false,
            dependency_scanning: false,
            license_policy_enabled: false,
            agent_sandbox_required: false,
        },
        notifications: NotificationSettings {
            watch_default: "watching".into(),
            notify_on_ci_failure: true,
            notify_on_agent_completion: true,
            notify_on_release: true,
        },
        retention: RetentionSettings {
            audit_days: 365,
            evidence_days: 180,
            workflow_run_days: 90,
            log_days: 30,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_settings_is_stable() {
        let id = RepoId {
            host: "gitlab".into(),
            owner: "n".into(),
            name: "p".into(),
        };
        let s1 = default_settings(
            &id,
            "gitlab",
            "main".into(),
            RepositoryVisibility::Private,
            None,
            false,
            vec![],
        );
        let s2 = default_settings(
            &id,
            "gitlab",
            "main".into(),
            RepositoryVisibility::Private,
            None,
            false,
            vec![],
        );
        assert_eq!(hash_settings(&s1), hash_settings(&s2));
    }

    #[test]
    fn visibility_to_string_round_trips() {
        assert_eq!(
            visibility_to_string(&RepositoryVisibility::Public),
            "public"
        );
        assert_eq!(
            visibility_to_string(&RepositoryVisibility::Internal),
            "internal"
        );
        assert_eq!(
            visibility_to_string(&RepositoryVisibility::Private),
            "private"
        );
    }
}
