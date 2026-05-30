//! Repository settings contracts (general/features/merge/CI/agents/…) plus
//! the settings patch + diff-preview wire formats consumed by the SPA's
//! settings editor.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::repository::{RepositoryId, RepositoryVisibility};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GeneralSettings {
    pub name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub visibility: RepositoryVisibility,
    pub default_branch: String,
    pub topics: Vec<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FeatureSettings {
    pub issues: bool,
    pub pull_requests: bool,
    pub wiki: bool,
    pub discussions: bool,
    pub projects: bool,
    pub packages: bool,
    pub releases: bool,
    pub ci: bool,
    pub security_advisories: bool,
    pub pages: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MergeSettings {
    pub allow_merge_commit: bool,
    pub allow_squash_merge: bool,
    pub allow_rebase_merge: bool,
    pub allow_auto_merge: bool,
    pub delete_branch_on_merge: bool,
    pub require_linear_history: bool,
    pub required_approvals: u32,
    pub dismiss_stale_approvals: bool,
    pub require_codeowners: bool,
    pub require_exact_sha_approval: bool,
    pub require_jeryu_merge_passport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BranchProtectionRule {
    pub pattern: String,
    pub required_checks: Vec<String>,
    pub required_approvals: u32,
    pub require_signed_commits: bool,
    pub require_conversation_resolution: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
    pub bypass_actors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CiSettings {
    pub default_runner_pool: Option<String>,
    pub concurrency_limit: Option<u32>,
    pub artifact_retention_days: u32,
    pub log_retention_days: u32,
    pub cache_retention_days: u32,
    pub vti_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentSettings {
    pub autonomous_coding_enabled: bool,
    pub max_concurrent_sessions: u32,
    pub require_human_approval_for_writes: bool,
    pub allowed_agents: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub evidence_required: bool,
    pub budget_daily_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AccessSettings {
    pub collaborators_count: u32,
    pub teams_count: u32,
    pub deploy_keys_count: u32,
    pub app_installations_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SecuritySettings {
    pub secret_scanning: bool,
    pub dependency_scanning: bool,
    pub license_policy_enabled: bool,
    pub agent_sandbox_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NotificationSettings {
    pub watch_default: String,
    pub notify_on_ci_failure: bool,
    pub notify_on_agent_completion: bool,
    pub notify_on_release: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RetentionSettings {
    pub audit_days: u32,
    pub evidence_days: u32,
    pub workflow_run_days: u32,
    pub log_days: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepositorySettings {
    pub repo: RepositoryId,
    pub general: GeneralSettings,
    pub features: FeatureSettings,
    pub merge: MergeSettings,
    pub branch_protection: Vec<BranchProtectionRule>,
    pub ci: CiSettings,
    pub agents: AgentSettings,
    pub access: AccessSettings,
    pub security: SecuritySettings,
    pub notifications: NotificationSettings,
    pub retention: RetentionSettings,
}

/// Settings patch wire-format. RFC 7396 merge semantics — only fields that
/// appear in the JSON body are updated; absent fields are left unchanged.
/// Mirrors the host adapter's `HostRepositorySettingsPatch` for the subset
/// of fields the BFF currently supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SettingsPatch {
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub visibility: Option<RepositoryVisibility>,
    pub default_branch: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SettingsFieldChange {
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Preview output: what would change if the patch were applied, plus a
/// bounded blast-radius summary for the currently supported patch surface.
/// The preview includes the affected branches/MRs and other downstream side
/// effects already known to this service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SettingsDiffPreview {
    pub repo: RepositoryId,
    pub current_hash: String,
    pub diffs: Vec<SettingsFieldChange>,
    pub side_effects: Vec<String>,
    pub warnings: Vec<String>,
    pub reversible: bool,
}
