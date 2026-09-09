//! Cache policy engine and phase-six cache laws.

mod engine;
#[cfg(test)]
mod tests;

pub use engine::CachePolicy;

use crate::ids::{RepoId, TenantId};
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
    pub(crate) fn allow(disposition: CacheDisposition, law: &str, reason: &str) -> Self {
        Self {
            allowed: true,
            disposition,
            law: law.to_string(),
            reason: reason.to_string(),
        }
    }

    pub(crate) fn deny(disposition: CacheDisposition, law: &str, reason: &str) -> Self {
        Self {
            allowed: false,
            disposition,
            law: law.to_string(),
            reason: reason.to_string(),
        }
    }
}
