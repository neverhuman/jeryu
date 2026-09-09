//! Network, secret, and token policies requested by a job.

use crate::error::{RunnerError, RunnerResult};

/// Network policy requested for the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    /// No network namespace connectivity.
    Deny,
    /// Loopback-only access.
    LoopbackOnly,
    /// Egress-only network access.
    EgressOnly,
}

impl NetworkPolicy {
    /// Stable policy label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::LoopbackOnly => "loopback-only",
            Self::EgressOnly => "egress-only",
        }
    }
}

impl std::str::FromStr for NetworkPolicy {
    type Err = RunnerError;

    fn from_str(value: &str) -> RunnerResult<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "deny" | "none" | "off" => Ok(Self::Deny),
            "loopback" | "loopback-only" => Ok(Self::LoopbackOnly),
            "egress" | "egress-only" => Ok(Self::EgressOnly),
            _ => Err(RunnerError::new(
                "invalid_network_policy",
                format!("unknown network policy '{value}'"),
            )),
        }
    }
}

/// Secret policy requested by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretPolicy {
    /// No secrets are provided.
    None,
    /// Use default tier-based secret policy.
    Default,
    /// Explicitly request secrets.
    Requested,
}

impl SecretPolicy {
    /// Stable policy label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Default => "default",
            Self::Requested => "requested",
        }
    }
}

impl std::str::FromStr for SecretPolicy {
    type Err = RunnerError;

    fn from_str(value: &str) -> RunnerResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" | "deny" | "false" => Ok(Self::None),
            "default" | "auto" => Ok(Self::Default),
            "requested" | "request" | "true" => Ok(Self::Requested),
            _ => Err(RunnerError::new(
                "invalid_secret_policy",
                format!("unknown secret policy '{value}'"),
            )),
        }
    }
}

/// Token policy for step-scoped credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPolicy {
    /// No token material.
    None,
    /// Read-only token.
    ReadOnly,
    /// Scoped write token.
    ScopedWrite,
}

impl TokenPolicy {
    /// Stable policy label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ReadOnly => "read-only",
            Self::ScopedWrite => "scoped-write",
        }
    }
}

impl std::str::FromStr for TokenPolicy {
    type Err = RunnerError;

    fn from_str(value: &str) -> RunnerResult<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "none" | "deny" => Ok(Self::None),
            "read" | "read-only" | "readonly" => Ok(Self::ReadOnly),
            "write" | "scoped-write" => Ok(Self::ScopedWrite),
            _ => Err(RunnerError::new(
                "invalid_token_policy",
                format!("unknown token policy '{value}'"),
            )),
        }
    }
}
