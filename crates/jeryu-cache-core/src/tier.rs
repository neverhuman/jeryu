use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Jeryu trust tiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustTier {
    T0ReleaseHermetic,
    T1ProtectedInternal,
    T2InternalBranch,
    T3AgentAuthored,
    T4ForkPr,
    T5PublicUntrusted,
}

impl TrustTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::T0ReleaseHermetic => "t0-release-hermetic",
            Self::T1ProtectedInternal => "t1-protected-internal",
            Self::T2InternalBranch => "t2-internal-branch",
            Self::T3AgentAuthored => "t3-agent-authored",
            Self::T4ForkPr => "t4-fork-pr",
            Self::T5PublicUntrusted => "t5-public-untrusted",
        }
    }

    pub fn can_write_trusted_compiled_cache(self) -> bool {
        matches!(self, Self::T1ProtectedInternal)
    }

    pub fn can_read_mutable_compiled_cache(self) -> bool {
        !matches!(
            self,
            Self::T0ReleaseHermetic | Self::T4ForkPr | Self::T5PublicUntrusted
        )
    }

    pub fn is_untrusted(self) -> bool {
        matches!(self, Self::T4ForkPr | Self::T5PublicUntrusted)
    }
}

impl fmt::Display for TrustTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TrustTier {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "t0-release-hermetic" | "T0" => Ok(Self::T0ReleaseHermetic),
            "t1-protected-internal" | "T1" => Ok(Self::T1ProtectedInternal),
            "t2-internal-branch" | "T2" => Ok(Self::T2InternalBranch),
            "t3-agent-authored" | "T3" => Ok(Self::T3AgentAuthored),
            "t4-fork-pr" | "T4" => Ok(Self::T4ForkPr),
            "t5-public-untrusted" | "T5" => Ok(Self::T5PublicUntrusted),
            other => Err(format!("unknown trust tier {other}")),
        }
    }
}
