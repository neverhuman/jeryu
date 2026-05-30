//! Cache policy engine and phase-six cache laws.

use crate::cache::{CacheKind, CacheScope};
use crate::ids::{RepoId, SharedScopeId, TenantId};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

/// Jeryu runner trust tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrustTier {
    /// T0 clean release lane.
    ReleaseHermetic,
    /// T1 protected internal branch.
    ProtectedInternal,
    /// T2 internal branch.
    InternalBranch,
    /// T3 agent-authored patch.
    AgentAuthored,
    /// T4 fork pull request.
    ForkPr,
    /// T5 public/untrusted job.
    PublicUntrusted,
}

impl TrustTier {
    /// Returns true for fork/public trust tiers.
    pub fn is_untrusted(self) -> bool {
        matches!(self, TrustTier::ForkPr | TrustTier::PublicUntrusted)
    }

    /// Returns true if the tier may promote trusted compiled artifacts after green CI.
    pub fn may_promote(self) -> bool {
        matches!(self, TrustTier::ProtectedInternal)
    }
}

impl Display for TrustTier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            TrustTier::ReleaseHermetic => "T0-release-hermetic",
            TrustTier::ProtectedInternal => "T1-protected-internal",
            TrustTier::InternalBranch => "T2-internal-branch",
            TrustTier::AgentAuthored => "T3-agent-authored",
            TrustTier::ForkPr => "T4-fork-pr",
            TrustTier::PublicUntrusted => "T5-public-untrusted",
        };
        f.write_str(value)
    }
}

/// Context supplied with every cache decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheContext {
    /// Tenant requesting cache access.
    pub tenant_id: TenantId,
    /// Repository requesting cache access.
    pub repo_id: RepoId,
    /// Trust tier of the job.
    pub trust_tier: TrustTier,
    /// True for release-hermetic lanes.
    pub release_lane: bool,
}

impl CacheContext {
    /// Creates a cache context.
    pub fn new(
        tenant_id: TenantId,
        repo_id: RepoId,
        trust_tier: TrustTier,
        release_lane: bool,
    ) -> Self {
        Self {
            tenant_id,
            repo_id,
            trust_tier,
            release_lane,
        }
    }
}

/// Cache operation to evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheAccess {
    /// Restore/read a cache entry.
    Read,
    /// Write a cache entry; `ci_green` means the producing lane passed.
    Write { ci_green: bool },
    /// Promote a quarantined entry into trusted cache.
    Promote { ci_green: bool },
}

/// Result disposition from policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheDisposition {
    /// Trusted cache hit/write is permitted.
    Trusted,
    /// Write is permitted only to quarantine.
    Quarantine,
    /// Read must safely miss instead of failing open.
    SafeMiss,
    /// Access is hard denied.
    Deny,
    /// Mutable compiled cache is ignored by release lanes.
    IgnoredMutableCache,
}

impl Display for CacheDisposition {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            CacheDisposition::Trusted => "trusted",
            CacheDisposition::Quarantine => "quarantine",
            CacheDisposition::SafeMiss => "safe-miss",
            CacheDisposition::Deny => "deny",
            CacheDisposition::IgnoredMutableCache => "ignored-mutable-cache",
        };
        f.write_str(value)
    }
}

/// Policy decision including the named law and explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    /// Whether the requested operation may proceed.
    pub allowed: bool,
    /// Effective disposition.
    pub disposition: CacheDisposition,
    /// Named cache law that produced the decision.
    pub law: String,
    /// Human-readable reason.
    pub reason: String,
}

impl PolicyDecision {
    fn allow(disposition: CacheDisposition, law: &str, reason: &str) -> Self {
        Self {
            allowed: true,
            disposition,
            law: law.to_string(),
            reason: reason.to_string(),
        }
    }

    fn deny(disposition: CacheDisposition, law: &str, reason: &str) -> Self {
        Self {
            allowed: false,
            disposition,
            law: law.to_string(),
            reason: reason.to_string(),
        }
    }
}

/// Phase 6 cache policy engine.
#[derive(Debug, Clone, Default)]
pub struct CachePolicy {
    explicit_shared_scopes: BTreeSet<(String, String)>,
}

impl CachePolicy {
    /// Creates a policy with no shared compiled artifact scopes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allows a shared compiled-artifact scope for a tenant.
    pub fn allow_shared_scope(&mut self, tenant_id: &TenantId, shared_scope_id: &SharedScopeId) {
        self.explicit_shared_scopes.insert((
            tenant_id.as_str().to_string(),
            shared_scope_id.as_str().to_string(),
        ));
    }

    /// Evaluates one cache operation.
    pub fn evaluate(
        &self,
        context: &CacheContext,
        scope: &CacheScope,
        kind: CacheKind,
        access: CacheAccess,
    ) -> PolicyDecision {
        if context.release_lane
            && kind == CacheKind::CompiledArtifact
            && !matches!(access, CacheAccess::Write { .. })
        {
            return PolicyDecision::deny(
                CacheDisposition::IgnoredMutableCache,
                "release-ignores-mutable-cache",
                "release lanes do not consume mutable compiled cache",
            );
        }

        if context.trust_tier == TrustTier::ReleaseHermetic && kind == CacheKind::CompiledArtifact {
            return PolicyDecision::deny(
                CacheDisposition::IgnoredMutableCache,
                "release-hermetic-no-compiled-cache",
                "T0 release hermetic jobs use vendor snapshots, not mutable compiled artifacts",
            );
        }

        if kind == CacheKind::ReleaseVendorSnapshot {
            return PolicyDecision::allow(
                CacheDisposition::Trusted,
                "release-vendor-snapshot",
                "release snapshots are immutable CAS inputs",
            );
        }

        if !self.scope_visible(context, scope, kind) {
            return PolicyDecision::deny(
                CacheDisposition::SafeMiss,
                "cross-project-read-denied-by-default",
                "compiled cache scope is not visible without an explicit shared scope",
            );
        }

        match access {
            CacheAccess::Read => self.evaluate_read(context, kind),
            CacheAccess::Write { ci_green } => self.evaluate_write(context, kind, ci_green),
            CacheAccess::Promote { ci_green } => self.evaluate_promote(context, kind, ci_green),
        }
    }

    fn evaluate_read(&self, context: &CacheContext, kind: CacheKind) -> PolicyDecision {
        if kind != CacheKind::CompiledArtifact {
            return PolicyDecision::allow(
                CacheDisposition::Trusted,
                "immutable-source-cache-readable",
                "source and registry blobs are content-addressed immutable inputs",
            );
        }

        if context.trust_tier.is_untrusted() {
            return PolicyDecision::deny(
                CacheDisposition::SafeMiss,
                "fork-source-cache-only",
                "untrusted jobs may read source cache only, not trusted compiled artifacts",
            );
        }

        PolicyDecision::allow(
            CacheDisposition::Trusted,
            "repo-scoped-compiled-read",
            "trusted non-release job may read visible compiled cache",
        )
    }

    fn evaluate_write(
        &self,
        context: &CacheContext,
        kind: CacheKind,
        ci_green: bool,
    ) -> PolicyDecision {
        if kind != CacheKind::CompiledArtifact {
            return PolicyDecision::allow(
                CacheDisposition::Trusted,
                "immutable-source-cache-write",
                "content-addressed source/registry writes are immutable",
            );
        }

        if context.trust_tier.is_untrusted() {
            return PolicyDecision::deny(
                CacheDisposition::Deny,
                "fork-pr-cannot-write-trusted-cache",
                "fork and public jobs cannot write compiled artifacts",
            );
        }

        if context.trust_tier == TrustTier::ProtectedInternal && ci_green {
            return PolicyDecision::allow(
                CacheDisposition::Trusted,
                "protected-green-cache-write",
                "green protected internal lane may write trusted compiled cache",
            );
        }

        PolicyDecision::allow(
            CacheDisposition::Quarantine,
            "cache-quarantine-before-promotion",
            "non-promotable or not-yet-green compiled cache writes enter quarantine",
        )
    }

    fn evaluate_promote(
        &self,
        context: &CacheContext,
        kind: CacheKind,
        ci_green: bool,
    ) -> PolicyDecision {
        if kind != CacheKind::CompiledArtifact {
            return PolicyDecision::allow(
                CacheDisposition::Trusted,
                "immutable-cache-needs-no-promotion",
                "source/registry blobs are already immutable CAS inputs",
            );
        }

        if context.trust_tier.may_promote() && ci_green {
            return PolicyDecision::allow(
                CacheDisposition::Trusted,
                "cache-promotion-after-green-protected-policy",
                "only green protected internal lanes may promote compiled cache",
            );
        }

        PolicyDecision::deny(
            CacheDisposition::Deny,
            "no-cache-promotion-without-receipt",
            "cache promotion requires green protected policy",
        )
    }

    fn scope_visible(&self, context: &CacheContext, scope: &CacheScope, kind: CacheKind) -> bool {
        match scope {
            CacheScope::Repo { tenant_id, repo_id } => {
                tenant_id == &context.tenant_id
                    && (repo_id == &context.repo_id || kind != CacheKind::CompiledArtifact)
            }
            CacheScope::Tenant { tenant_id } => {
                tenant_id == &context.tenant_id && kind != CacheKind::CompiledArtifact
            }
            CacheScope::Shared {
                tenant_id,
                shared_scope_id,
            } => {
                tenant_id == &context.tenant_id
                    && self.explicit_shared_scopes.contains(&(
                        tenant_id.as_str().to_string(),
                        shared_scope_id.as_str().to_string(),
                    ))
                    && !context.trust_tier.is_untrusted()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheKind;

    fn ctx(tier: TrustTier) -> CacheContext {
        CacheContext::new(
            TenantId::new("tenant").expect("valid tenant"),
            RepoId::new("repo-a").expect("valid repo"),
            tier,
            false,
        )
    }

    fn repo_scope(name: &str) -> CacheScope {
        CacheScope::Repo {
            tenant_id: TenantId::new("tenant").expect("valid tenant"),
            repo_id: RepoId::new(name).expect("valid repo"),
        }
    }

    #[test]
    fn policy_denies_cross_project_compiled_read() {
        let policy = CachePolicy::new();
        let decision = policy.evaluate(
            &ctx(TrustTier::InternalBranch),
            &repo_scope("repo-b"),
            CacheKind::CompiledArtifact,
            CacheAccess::Read,
        );
        assert_eq!(decision.disposition, CacheDisposition::SafeMiss);
        assert!(!decision.allowed);
    }

    #[test]
    fn policy_denies_fork_compiled_write() {
        let policy = CachePolicy::new();
        let decision = policy.evaluate(
            &ctx(TrustTier::ForkPr),
            &repo_scope("repo-a"),
            CacheKind::CompiledArtifact,
            CacheAccess::Write { ci_green: true },
        );
        assert_eq!(decision.disposition, CacheDisposition::Deny);
        assert!(!decision.allowed);
    }
}
