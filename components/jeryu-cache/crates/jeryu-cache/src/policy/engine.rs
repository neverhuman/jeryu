//! Phase 6 cache policy engine implementation.

use super::{CacheAccess, CacheContext, CacheDisposition, PolicyDecision, TrustTier};
use crate::cache::{CacheKind, CacheScope};
use crate::ids::{SharedScopeId, TenantId};
use std::collections::BTreeSet;

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
