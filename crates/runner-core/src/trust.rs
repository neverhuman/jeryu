#![doc = "Trust tiers and runner classes."]

use crate::error::{RunnerError, RunnerResult};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// JitForge trust tier for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrustTier {
    /// T0 release-hermetic: clean, signed, no mutable cache.
    T0ReleaseHermetic,
    /// T1 protected-internal: native-hot allowed, trusted cache writes after green.
    T1ProtectedInternal,
    /// T2 internal branch: native-clean allowed, limited cache writes.
    T2InternalBranch,
    /// T3 agent-authored: agent guard, proof required, scoped writes.
    T3AgentAuthored,
    /// T4 fork PR: microVM or OCI, no trusted cache writes.
    T4ForkPr,
    /// T5 public/untrusted: microVM only, no secrets by default.
    T5PublicUntrusted,
}

impl TrustTier {
    /// Stable tier label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::T0ReleaseHermetic => "T0_RELEASE_HERMETIC",
            Self::T1ProtectedInternal => "T1_PROTECTED_INTERNAL",
            Self::T2InternalBranch => "T2_INTERNAL_BRANCH",
            Self::T3AgentAuthored => "T3_AGENT_AUTHORED",
            Self::T4ForkPr => "T4_FORK_PR",
            Self::T5PublicUntrusted => "T5_PUBLIC_UNTRUSTED",
        }
    }

    /// True when the tier is trusted enough for native execution.
    pub fn permits_native(self) -> bool {
        matches!(
            self,
            Self::T0ReleaseHermetic
                | Self::T1ProtectedInternal
                | Self::T2InternalBranch
                | Self::T3AgentAuthored
        )
    }

    /// True when secrets may be provided by default.
    pub fn permits_default_secrets(self) -> bool {
        matches!(
            self,
            Self::T0ReleaseHermetic | Self::T1ProtectedInternal | Self::T2InternalBranch
        )
    }

    /// True when trusted compiled cache writes are possible after a green policy.
    pub fn permits_trusted_cache_write(self) -> bool {
        matches!(self, Self::T0ReleaseHermetic | Self::T1ProtectedInternal)
    }
}

impl Display for TrustTier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TrustTier {
    type Err = RunnerError;

    fn from_str(value: &str) -> RunnerResult<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "t0" | "t0_release_hermetic" | "release_hermetic" => Ok(Self::T0ReleaseHermetic),
            "t1" | "t1_protected_internal" | "protected_internal" => Ok(Self::T1ProtectedInternal),
            "t2" | "t2_internal_branch" | "internal_branch" => Ok(Self::T2InternalBranch),
            "t3" | "t3_agent_authored" | "agent_authored" => Ok(Self::T3AgentAuthored),
            "t4" | "t4_fork_pr" | "fork_pr" => Ok(Self::T4ForkPr),
            "t5" | "t5_public_untrusted" | "public_untrusted" => Ok(Self::T5PublicUntrusted),
            _ => Err(RunnerError::new(
                "invalid_trust_tier",
                format!("unknown trust tier '{value}'"),
            )),
        }
    }
}

/// Concrete runner class chosen by the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunnerClass {
    /// Pre-warmed trusted native sandbox for hot Rust PR loops.
    NativeRustHot,
    /// Fresh trusted native sandbox.
    NativeRustClean,
    /// Native agent-authored repair loop with proof guard.
    AgentGuard,
    /// Clean release lane.
    ReleaseHermetic,
    /// MicroVM lane for forks and untrusted jobs.
    MicroVmRust,
    /// OCI/Docker/Podman compatibility lane.
    OciDocker,
}

impl RunnerClass {
    /// Stable class label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeRustHot => "native-rust-hot",
            Self::NativeRustClean => "native-rust-clean",
            Self::AgentGuard => "agent-guard",
            Self::ReleaseHermetic => "release-hermetic",
            Self::MicroVmRust => "microvm-rust",
            Self::OciDocker => "oci-docker",
        }
    }

    /// True when this class executes directly on the native runner fabric.
    pub fn is_native(self) -> bool {
        matches!(
            self,
            Self::NativeRustHot | Self::NativeRustClean | Self::AgentGuard | Self::ReleaseHermetic
        )
    }
}

impl Display for RunnerClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunnerClass {
    type Err = RunnerError;

    fn from_str(value: &str) -> RunnerResult<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
        match normalized.as_str() {
            "native-rust-hot" => Ok(Self::NativeRustHot),
            "native-rust-clean" => Ok(Self::NativeRustClean),
            "agent-guard" => Ok(Self::AgentGuard),
            "release-hermetic" => Ok(Self::ReleaseHermetic),
            "microvm-rust" => Ok(Self::MicroVmRust),
            "oci-docker" | "oci" | "docker" | "podman" => Ok(Self::OciDocker),
            _ => Err(RunnerError::new(
                "invalid_runner_class",
                format!("unknown runner class '{value}'"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_tiers_parse_aliases() {
        let parsed: TrustTier = "fork-pr".parse().unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(parsed, TrustTier::T4ForkPr);
    }

    #[test]
    fn untrusted_tiers_do_not_permit_native() {
        assert!(!TrustTier::T4ForkPr.permits_native());
        assert!(!TrustTier::T5PublicUntrusted.permits_native());
    }
}
