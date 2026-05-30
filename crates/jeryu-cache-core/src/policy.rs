use serde::{Deserialize, Serialize};

use crate::tier::TrustTier;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheAction {
    Read,
    Write,
    Restore,
    Promote,
    QuarantineWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheLayer {
    L0JobTmpfs,
    L1RunnerLocalProject,
    L2RunnerLocalSourceBlob,
    L3RepoCompiledCas,
    L4TenantSourceCas,
    L5ExplicitSharedCompiledCas,
    L6ReleaseHermeticVendorSnapshot,
}

impl CacheLayer {
    pub fn is_compiled(self) -> bool {
        matches!(
            self,
            Self::L1RunnerLocalProject
                | Self::L3RepoCompiledCas
                | Self::L5ExplicitSharedCompiledCas
        )
    }

    pub fn is_mutable(self) -> bool {
        !matches!(self, Self::L6ReleaseHermeticVendorSnapshot)
    }

    pub fn is_trusted_compiled(self) -> bool {
        matches!(
            self,
            Self::L1RunnerLocalProject
                | Self::L3RepoCompiledCas
                | Self::L5ExplicitSharedCompiledCas
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::L0JobTmpfs => "L0-job-tmpfs",
            Self::L1RunnerLocalProject => "L1-runner-local-project-cache",
            Self::L2RunnerLocalSourceBlob => "L2-runner-local-source-blob-cache",
            Self::L3RepoCompiledCas => "L3-repo-compiled-cas",
            Self::L4TenantSourceCas => "L4-tenant-source-cas",
            Self::L5ExplicitSharedCompiledCas => "L5-explicit-shared-compiled-cas",
            Self::L6ReleaseHermeticVendorSnapshot => "L6-release-hermetic-vendor-snapshot",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CacheScope {
    Job {
        job_id: String,
    },
    Repo {
        tenant_id: String,
        repo_id: String,
    },
    Tenant {
        tenant_id: String,
    },
    ExplicitShared {
        tenant_id: String,
        scope_id: String,
        allowlisted: bool,
    },
    ReleaseHermetic {
        tenant_id: String,
        repo_id: String,
        snapshot_id: String,
    },
}

impl CacheScope {
    pub fn tenant_id(&self) -> &str {
        match self {
            Self::Job { job_id } => job_id,
            Self::Repo { tenant_id, .. }
            | Self::Tenant { tenant_id }
            | Self::ExplicitShared { tenant_id, .. }
            | Self::ReleaseHermetic { tenant_id, .. } => tenant_id,
        }
    }

    pub fn repo_id(&self) -> Option<&str> {
        match self {
            Self::Repo { repo_id, .. } | Self::ReleaseHermetic { repo_id, .. } => Some(repo_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheRequest {
    pub action: CacheAction,
    pub layer: CacheLayer,
    pub actor_tier: TrustTier,
    pub source_repo_id: String,
    pub target_repo_id: String,
    pub scope: CacheScope,
    pub green_protected_policy: bool,
    pub has_explainable_fingerprint: bool,
    pub has_receipt: bool,
    pub is_release_lane: bool,
    pub is_agent_patch: bool,
}

impl CacheRequest {
    pub fn same_repo(&self) -> bool {
        self.source_repo_id == self.target_repo_id
    }

    pub fn explicit_shared_allowed(&self) -> bool {
        matches!(
            &self.scope,
            CacheScope::ExplicitShared {
                allowlisted: true,
                ..
            }
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum AccessDecision {
    Allow { reasons: Vec<String> },
    Deny { reasons: Vec<String> },
}

impl AccessDecision {
    pub fn allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    pub fn reasons(&self) -> &[String] {
        match self {
            Self::Allow { reasons } | Self::Deny { reasons } => reasons,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PolicyEngine;

impl PolicyEngine {
    pub fn evaluate(&self, request: &CacheRequest) -> AccessDecision {
        let mut deny = Vec::<String>::new();
        let mut allow = Vec::<String>::new();

        if matches!(request.action, CacheAction::Restore | CacheAction::Read)
            && !request.has_explainable_fingerprint
        {
            deny.push("CV-LAW-008: restore/read requires explainable fingerprint".into());
        }

        if request.is_release_lane
            && request.layer.is_compiled()
            && request.layer.is_mutable()
            && matches!(request.action, CacheAction::Read | CacheAction::Restore)
        {
            deny.push("CV-LAW-004: release lane cannot consume mutable compiled cache".into());
        }

        if matches!(request.actor_tier, TrustTier::T0ReleaseHermetic)
            && !matches!(request.layer, CacheLayer::L6ReleaseHermeticVendorSnapshot)
            && matches!(request.action, CacheAction::Read | CacheAction::Restore)
        {
            deny.push("T0 release-hermetic lanes may restore only L6 hermetic snapshots".into());
        }

        if request.actor_tier.is_untrusted()
            && request.layer.is_trusted_compiled()
            && matches!(request.action, CacheAction::Read | CacheAction::Restore)
        {
            deny.push("CV-LAW-001/CV-LAW-003: untrusted jobs may read source caches only, not trusted compiled cache".into());
        }

        if request.actor_tier.is_untrusted()
            && request.layer.is_trusted_compiled()
            && matches!(request.action, CacheAction::Write | CacheAction::Promote)
        {
            deny.push(
                "CV-LAW-002/CV-LAW-003: untrusted jobs cannot write trusted compiled cache".into(),
            );
        }

        if matches!(request.action, CacheAction::Promote) && !request.has_receipt {
            deny.push("CV-LAW-009: cache promotion requires receipt".into());
        }

        if matches!(request.action, CacheAction::Write | CacheAction::Promote)
            && request.layer.is_trusted_compiled()
            && !request.actor_tier.can_write_trusted_compiled_cache()
        {
            deny.push("only T1 protected-internal can write trusted compiled cache".into());
        }

        if matches!(request.action, CacheAction::Write | CacheAction::Promote)
            && request.layer.is_trusted_compiled()
            && request.actor_tier.can_write_trusted_compiled_cache()
            && !request.green_protected_policy
        {
            deny.push("trusted compiled cache write requires green protected policy".into());
        }

        if request.layer == CacheLayer::L5ExplicitSharedCompiledCas
            && !request.explicit_shared_allowed()
        {
            deny.push("CV-LAW-005: cross-project compiled artifact sharing denied without explicit allowlist".into());
        }

        if request.layer.is_compiled()
            && !request.same_repo()
            && request.layer != CacheLayer::L5ExplicitSharedCompiledCas
        {
            deny.push("CV-LAW-005: compiled artifacts are repo-scoped by default".into());
        }

        if request.is_agent_patch
            && request.layer.is_trusted_compiled()
            && matches!(request.action, CacheAction::Write | CacheAction::Promote)
        {
            deny.push("agent patches write quarantine cache only until proof receipt".into());
        }

        if deny.is_empty() {
            allow.push(format!(
                "{} {:?} permitted for {}",
                request.layer.as_str(),
                request.action,
                request.actor_tier
            ));
            if request.layer.is_compiled() {
                allow.push(
                    "compiled cache access remains scoped by key material fingerprint".into(),
                );
            }
            AccessDecision::Allow { reasons: allow }
        } else {
            AccessDecision::Deny { reasons: deny }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(action: CacheAction, layer: CacheLayer, actor_tier: TrustTier) -> CacheRequest {
        CacheRequest {
            action,
            layer,
            actor_tier,
            source_repo_id: "repo-a".into(),
            target_repo_id: "repo-a".into(),
            scope: CacheScope::Repo {
                tenant_id: "tenant".into(),
                repo_id: "repo-a".into(),
            },
            green_protected_policy: true,
            has_explainable_fingerprint: true,
            has_receipt: true,
            is_release_lane: false,
            is_agent_patch: false,
        }
    }

    #[test]
    fn policy_denies_fork_write_to_trusted_cache() {
        let req = base(
            CacheAction::Write,
            CacheLayer::L3RepoCompiledCas,
            TrustTier::T4ForkPr,
        );
        assert!(!PolicyEngine.evaluate(&req).allowed());
    }

    #[test]
    fn policy_denies_release_mutable_compiled_restore() {
        let mut req = base(
            CacheAction::Restore,
            CacheLayer::L3RepoCompiledCas,
            TrustTier::T0ReleaseHermetic,
        );
        req.is_release_lane = true;
        assert!(!PolicyEngine.evaluate(&req).allowed());
    }

    #[test]
    fn policy_denies_restore_without_fingerprint() {
        let mut req = base(
            CacheAction::Restore,
            CacheLayer::L2RunnerLocalSourceBlob,
            TrustTier::T2InternalBranch,
        );
        req.has_explainable_fingerprint = false;
        assert!(!PolicyEngine.evaluate(&req).allowed());
    }

    #[test]
    fn policy_denies_fork_restore_from_compiled_cache() {
        let req = base(
            CacheAction::Restore,
            CacheLayer::L3RepoCompiledCas,
            TrustTier::T4ForkPr,
        );
        assert!(!PolicyEngine.evaluate(&req).allowed());
    }

    #[test]
    fn policy_allows_fork_restore_from_source_cache() {
        let req = base(
            CacheAction::Restore,
            CacheLayer::L2RunnerLocalSourceBlob,
            TrustTier::T4ForkPr,
        );
        assert!(PolicyEngine.evaluate(&req).allowed());
    }

    #[test]
    fn policy_allows_t1_green_write() {
        let req = base(
            CacheAction::Write,
            CacheLayer::L3RepoCompiledCas,
            TrustTier::T1ProtectedInternal,
        );
        assert!(PolicyEngine.evaluate(&req).allowed());
    }

    #[test]
    fn policy_denies_cross_project_compiled_by_default() {
        let mut req = base(
            CacheAction::Read,
            CacheLayer::L3RepoCompiledCas,
            TrustTier::T2InternalBranch,
        );
        req.target_repo_id = "repo-b".into();
        assert!(!PolicyEngine.evaluate(&req).allowed());
    }

    #[test]
    fn policy_denies_explicit_shared_compiled_without_allowlist_even_same_repo() {
        let mut req = base(
            CacheAction::Read,
            CacheLayer::L5ExplicitSharedCompiledCas,
            TrustTier::T2InternalBranch,
        );
        req.scope = CacheScope::ExplicitShared {
            tenant_id: "tenant".into(),
            scope_id: "shared-compiled".into(),
            allowlisted: false,
        };

        let decision = PolicyEngine.evaluate(&req);

        assert!(!decision.allowed());
        assert!(
            decision
                .reasons()
                .iter()
                .any(|reason| reason.contains("CV-LAW-005"))
        );
    }

    #[test]
    fn policy_allows_explicit_shared_compiled_when_allowlisted() {
        let mut req = base(
            CacheAction::Read,
            CacheLayer::L5ExplicitSharedCompiledCas,
            TrustTier::T2InternalBranch,
        );
        req.scope = CacheScope::ExplicitShared {
            tenant_id: "tenant".into(),
            scope_id: "shared-compiled".into(),
            allowlisted: true,
        };

        assert!(PolicyEngine.evaluate(&req).allowed());
    }
}
