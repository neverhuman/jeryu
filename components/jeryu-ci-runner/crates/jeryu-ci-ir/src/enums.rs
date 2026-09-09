//! Enumerated IR value types: pipeline sources, trust tiers, runner classes,
//! and the network/token/cache/artifact policy enums.

use std::fmt;
use std::str::FromStr;

use crate::hashing::normalize_token;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PipelineSource {
    GitHubActions,
    NativeToml,
    Api,
    Agent,
    MergeQueue,
    Hotfix,
    Release,
    Scheduled,
    Unknown(String),
}

impl PipelineSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::GitHubActions => "github-actions",
            Self::NativeToml => "jit-native",
            Self::Api => "api",
            Self::Agent => "agent",
            Self::MergeQueue => "merge-queue",
            Self::Hotfix => "hotfix",
            Self::Release => "release",
            Self::Scheduled => "scheduled",
            Self::Unknown(value) => value.as_str(),
        }
    }
}

impl fmt::Display for PipelineSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustTier {
    ReleaseHermetic,
    ProtectedInternal,
    InternalBranch,
    AgentAuthored,
    ForkPr,
    PublicUntrusted,
}

impl TrustTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReleaseHermetic => "T0-release-hermetic",
            Self::ProtectedInternal => "T1-protected-internal",
            Self::InternalBranch => "T2-internal-branch",
            Self::AgentAuthored => "T3-agent-authored",
            Self::ForkPr => "T4-fork-pr",
            Self::PublicUntrusted => "T5-public-untrusted",
        }
    }
}

impl FromStr for TrustTier {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_token(value).as_str() {
            "t0releasehermetic" | "releasehermetic" | "release" => Ok(Self::ReleaseHermetic),
            "t1protectedinternal" | "protectedinternal" | "protected" => {
                Ok(Self::ProtectedInternal)
            }
            "t2internalbranch" | "internalbranch" | "internal" => Ok(Self::InternalBranch),
            "t3agentauthored" | "agentauthored" | "agent" => Ok(Self::AgentAuthored),
            "t4forkpr" | "forkpr" | "fork" => Ok(Self::ForkPr),
            "t5publicuntrusted" | "publicuntrusted" | "public" | "untrusted" => {
                Ok(Self::PublicUntrusted)
            }
            other => Err(format!("unknown trust tier: {other}")),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum RunnerClass {
    NativeRustHot,
    #[default]
    NativeRustClean,
    CrategraphDelta,
    NextestCapsule,
    AgentGuard,
    MergeSpec,
    ReleaseHermetic,
    MicrovmRust,
    OciDocker,
    K8sOci,
    Custom(String),
}

impl RunnerClass {
    pub fn as_str(&self) -> &str {
        match self {
            Self::NativeRustHot => "native-rust-hot",
            Self::NativeRustClean => "native-rust-clean",
            Self::CrategraphDelta => "crategraph-delta",
            Self::NextestCapsule => "nextest-capsule",
            Self::AgentGuard => "agent-guard",
            Self::MergeSpec => "merge-spec",
            Self::ReleaseHermetic => "release-hermetic",
            Self::MicrovmRust => "microvm-rust",
            Self::OciDocker => "oci-docker",
            Self::K8sOci => "k8s-oci",
            Self::Custom(value) => value.as_str(),
        }
    }
}

impl FromStr for RunnerClass {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = crate::trim_quotes(value).trim();
        match normalize_token(trimmed).as_str() {
            "nativerusthot" => Ok(Self::NativeRustHot),
            "nativerustclean" | "ubuntulatest" | "linux" => Ok(Self::NativeRustClean),
            "crategraphdelta" => Ok(Self::CrategraphDelta),
            "nextestcapsule" => Ok(Self::NextestCapsule),
            "agentguard" => Ok(Self::AgentGuard),
            "mergespec" => Ok(Self::MergeSpec),
            "releasehermetic" => Ok(Self::ReleaseHermetic),
            "microvmrust" | "microvm" => Ok(Self::MicrovmRust),
            "ocidocker" | "docker" => Ok(Self::OciDocker),
            "k8soci" | "kubernetes" | "k8s" => Ok(Self::K8sOci),
            _ if !trimmed.is_empty() => Ok(Self::Custom(trimmed.to_string())),
            _ => Err("runner class cannot be empty".to_string()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NetworkPolicy {
    #[default]
    Deny,
    Allowlist(Vec<String>),
    Open,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TokenScope {
    None,
    #[default]
    ReadRepo,
    WriteChecks,
    WritePullRequest,
    Custom(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheMode {
    ReadOnly,
    ReadWriteQuarantine,
    ReadWriteTrusted,
}

impl CacheMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::ReadWriteQuarantine => "read-write-quarantine",
            Self::ReadWriteTrusted => "read-write-trusted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactWhen {
    Always,
    OnSuccess,
    OnFailure,
}

impl ArtifactWhen {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::OnSuccess => "on-success",
            Self::OnFailure => "on-failure",
        }
    }
}
