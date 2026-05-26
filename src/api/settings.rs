//! Owner: TUI Control-Plane API + Web Forge BFF — repository settings DTOs
//! Proof: `cargo nextest run -p jeryu --lib api`
//! Invariants: API contract types only; no I/O, no domain side effects.
//!
//! Source: FINAL spec §6.2. The settings tree is fully typed; only
//! host-specific extension blocks would use opaque `serde_json::Value`,
//! and we keep none in v1.

use serde::{Deserialize, Serialize};

use super::repository::{RepositoryId, RepositoryVisibility};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct GeneralSettings {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub visibility: RepositoryVisibility,
    pub default_branch: String,
    pub topics: Vec<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct FeatureSettings {
    pub issues: bool,
    pub merge_requests: bool,
    pub wiki: bool,
    pub discussions: bool,
    pub projects: bool,
    pub packages: bool,
    pub releases: bool,
    pub ci: bool,
    pub security_advisories: bool,
    pub pages: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct CiSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_runner_pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency_limit: Option<u32>,
    pub artifact_retention_days: u32,
    pub log_retention_days: u32,
    pub cache_retention_days: u32,
    pub vti_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct AgentSettings {
    pub autonomous_coding_enabled: bool,
    pub max_concurrent_sessions: u32,
    pub require_human_approval_for_writes: bool,
    pub allowed_agents: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub evidence_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_daily_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct AccessSettings {
    pub collaborators_count: u32,
    pub teams_count: u32,
    pub deploy_keys_count: u32,
    pub app_installations_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct SecuritySettings {
    pub secret_scanning: bool,
    pub dependency_scanning: bool,
    pub license_policy_enabled: bool,
    pub agent_sandbox_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct NotificationSettings {
    pub watch_default: String,
    pub notify_on_ci_failure: bool,
    pub notify_on_agent_completion: bool,
    pub notify_on_release: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct RetentionSettings {
    pub audit_days: u32,
    pub evidence_days: u32,
    pub workflow_run_days: u32,
    pub log_days: u32,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RepositorySettings {
        RepositorySettings {
            repo: RepositoryId {
                id: "repo-1".into(),
                host: "gitlab".into(),
                owner: "neverhuman".into(),
                name: "jeryu".into(),
            },
            general: GeneralSettings {
                name: "jeryu".into(),
                description: Some("control plane".into()),
                homepage: None,
                visibility: RepositoryVisibility::Internal,
                default_branch: "main".into(),
                topics: vec!["ci".into()],
                archived: false,
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
                security_advisories: true,
                pages: false,
            },
            merge: MergeSettings {
                allow_merge_commit: true,
                allow_squash_merge: true,
                allow_rebase_merge: false,
                allow_auto_merge: false,
                delete_branch_on_merge: true,
                require_linear_history: false,
                required_approvals: 2,
                dismiss_stale_approvals: true,
                require_codeowners: true,
                require_exact_sha_approval: true,
                require_jeryu_merge_passport: true,
            },
            branch_protection: vec![BranchProtectionRule {
                pattern: "main".into(),
                required_checks: vec!["ci/build".into()],
                required_approvals: 2,
                require_signed_commits: true,
                require_conversation_resolution: true,
                allow_force_pushes: false,
                allow_deletions: false,
                bypass_actors: vec![],
            }],
            ci: CiSettings {
                default_runner_pool: Some("standard".into()),
                concurrency_limit: Some(10),
                artifact_retention_days: 30,
                log_retention_days: 30,
                cache_retention_days: 7,
                vti_enabled: true,
            },
            agents: AgentSettings {
                autonomous_coding_enabled: true,
                max_concurrent_sessions: 4,
                require_human_approval_for_writes: true,
                allowed_agents: vec!["claude".into()],
                allowed_tools: vec!["bash".into()],
                evidence_required: true,
                budget_daily_usd: Some(50.0),
            },
            access: AccessSettings::default(),
            security: SecuritySettings {
                secret_scanning: true,
                dependency_scanning: true,
                license_policy_enabled: true,
                agent_sandbox_required: true,
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

    #[test]
    fn repository_settings_roundtrips() {
        let s = sample();
        let json = serde_json::to_string(&s).unwrap();
        let back: RepositorySettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.general.name, "jeryu");
        assert_eq!(back.merge.required_approvals, 2);
        assert_eq!(back.branch_protection.len(), 1);
        assert!(back.merge.require_jeryu_merge_passport);
        assert!(matches!(
            back.general.visibility,
            RepositoryVisibility::Internal
        ));
    }

    #[test]
    fn access_settings_default_is_zero() {
        let a = AccessSettings::default();
        assert_eq!(a.collaborators_count, 0);
        assert_eq!(a.teams_count, 0);
    }

    #[test]
    fn settings_omits_none_optionals() {
        let mut s = sample();
        s.general.homepage = None;
        s.ci.default_runner_pool = None;
        s.ci.concurrency_limit = None;
        s.agents.budget_daily_usd = None;
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("homepage"));
        assert!(!json.contains("default_runner_pool"));
        assert!(!json.contains("concurrency_limit"));
        assert!(!json.contains("budget_daily_usd"));
    }
}
