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
