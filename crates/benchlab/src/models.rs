//! Core benchmark model.

use std::fmt;

/// Systems or runner modes JitForge is compared against.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Competitor {
    BaselineRunnerContainer,
    BaselineRunnerShell,
    BaselineRunnerKubernetes,
    GitHubActionsSelfHosted,
    GiteaActions,
    ForgejoActions,
}

impl Competitor {
    /// Stable slug used in receipts.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::BaselineRunnerContainer => "baseline-runner-container",
            Self::BaselineRunnerShell => "baseline-runner-shell",
            Self::BaselineRunnerKubernetes => "baseline-runner-kubernetes",
            Self::GitHubActionsSelfHosted => "github-actions-self-hosted",
            Self::GiteaActions => "gitea-actions",
            Self::ForgejoActions => "forgejo-actions",
        }
    }
}

impl fmt::Display for Competitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// JitForge execution modes included in Phase 10 scorecards.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum JitForgeRunner {
    NativeRustHot,
    NativeRustClean,
    MicroVmRust,
    OciDocker,
    K8sOci,
}

impl JitForgeRunner {
    /// Stable slug used in receipts.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::NativeRustHot => "native-rust-hot",
            Self::NativeRustClean => "native-rust-clean",
            Self::MicroVmRust => "microvm-rust",
            Self::OciDocker => "oci-docker",
            Self::K8sOci => "k8s-oci",
        }
    }
}

impl fmt::Display for JitForgeRunner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// Workload family used by benchmark targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ScenarioClass {
    TrustedNoOpRustPr,
    SmallWarmRustPr,
    MediumAffectedCratePr,
    LargeAffectedCratePr,
    TestOnlySharded,
    MergeQueueMedian,
    CacheSafety,
    AgentHotfix,
}

impl ScenarioClass {
    /// Stable slug used in receipts and fixtures.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::TrustedNoOpRustPr => "trusted-no-op-rust-pr",
            Self::SmallWarmRustPr => "small-warm-rust-pr",
            Self::MediumAffectedCratePr => "medium-affected-crate-pr",
            Self::LargeAffectedCratePr => "large-affected-crate-pr",
            Self::TestOnlySharded => "test-only-sharded",
            Self::MergeQueueMedian => "merge-queue-median",
            Self::CacheSafety => "cache-safety",
            Self::AgentHotfix => "agent-hotfix",
        }
    }
}

/// Cache state for a replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CacheState {
    Cold,
    WarmProjectCache,
    WarmTenantSourceCache,
    Quarantine,
    ReleaseHermetic,
}

impl CacheState {
    /// Stable slug used in receipts.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Cold => "cold",
            Self::WarmProjectCache => "warm_project_cache",
            Self::WarmTenantSourceCache => "warm_tenant_source_cache",
            Self::Quarantine => "quarantine",
            Self::ReleaseHermetic => "release_hermetic",
        }
    }
}

/// Security trust tier.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TrustTier {
    ReleaseHermetic,
    ProtectedInternal,
    InternalBranch,
    AgentAuthored,
    ForkPr,
    PublicUntrusted,
}

impl TrustTier {
    /// Stable slug used in receipts.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::ReleaseHermetic => "release_hermetic",
            Self::ProtectedInternal => "protected_internal",
            Self::InternalBranch => "internal_branch",
            Self::AgentAuthored => "agent_authored",
            Self::ForkPr => "fork_pr",
            Self::PublicUntrusted => "public_untrusted",
        }
    }
}

/// Deterministic non-cryptographic digest for fixtures and test receipts.
///
/// Production SignRail replaces this with BLAKE3/Sigstore-backed attestations.
pub fn stable_digest(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64:{hash:016x}")
}

/// Escape a string for minimal JSON emission.
pub fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
