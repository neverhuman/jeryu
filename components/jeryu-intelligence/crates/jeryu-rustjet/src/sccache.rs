#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustTier {
    ReleaseHermetic,
    ProtectedInternal,
    InternalBranch,
    AgentAuthored,
    ForkPullRequest,
    PublicUntrusted,
}

impl TrustTier {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReleaseHermetic => "T0 release-hermetic",
            Self::ProtectedInternal => "T1 protected-internal",
            Self::InternalBranch => "T2 internal-branch",
            Self::AgentAuthored => "T3 agent-authored",
            Self::ForkPullRequest => "T4 fork-pr",
            Self::PublicUntrusted => "T5 public-untrusted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SccacheMode {
    Disabled,
    ReadOnly,
    ReadWriteProject,
    QuarantineWrite,
}

impl SccacheMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::ReadOnly => "read-only",
            Self::ReadWriteProject => "read-write-project",
            Self::QuarantineWrite => "quarantine-write",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SccacheDecision {
    pub mode: SccacheMode,
    pub reason: String,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SccachePolicy {
    pub project_scope: String,
    pub allow_agent_quarantine: bool,
}

impl Default for SccachePolicy {
    fn default() -> Self {
        Self {
            project_scope: "repo".to_string(),
            allow_agent_quarantine: true,
        }
    }
}

impl SccachePolicy {
    #[must_use]
    pub fn decide(&self, trust_tier: TrustTier, release_lane: bool) -> SccacheDecision {
        if release_lane || trust_tier == TrustTier::ReleaseHermetic {
            return SccacheDecision {
                mode: SccacheMode::Disabled,
                reason: "release/hermetic lanes do not consume mutable compiled caches".to_string(),
                env: Vec::new(),
            };
        }

        match trust_tier {
            TrustTier::ProtectedInternal | TrustTier::InternalBranch => SccacheDecision {
                mode: SccacheMode::ReadWriteProject,
                reason: "trusted internal lane may read/write project-scoped compiled cache after green policy".to_string(),
                env: vec![
                    ("RUSTC_WRAPPER".to_string(), "sccache".to_string()),
                    ("JERYU_CACHE_SCOPE".to_string(), self.project_scope.clone()),
                    ("JERYU_CACHE_PROMOTION".to_string(), "after-green".to_string()),
                ],
            },
            TrustTier::AgentAuthored if self.allow_agent_quarantine => SccacheDecision {
                mode: SccacheMode::QuarantineWrite,
                reason: "agent-authored lanes can write only quarantine cache entries".to_string(),
                env: vec![
                    ("RUSTC_WRAPPER".to_string(), "sccache".to_string()),
                    ("JERYU_CACHE_SCOPE".to_string(), "quarantine".to_string()),
                    ("JERYU_CACHE_PROMOTION".to_string(), "never-without-receipt".to_string()),
                ],
            },
            TrustTier::AgentAuthored | TrustTier::ForkPullRequest | TrustTier::PublicUntrusted => SccacheDecision {
                mode: SccacheMode::ReadOnly,
                reason: "untrusted or bounded lanes cannot write trusted compiled cache".to_string(),
                env: vec![
                    ("RUSTC_WRAPPER".to_string(), "sccache".to_string()),
                    ("JERYU_CACHE_READ_ONLY".to_string(), "1".to_string()),
                ],
            },
            TrustTier::ReleaseHermetic => SccacheDecision { mode: SccacheMode::Disabled, reason: "release/hermetic lanes do not consume mutable compiled caches".to_string(), env: Vec::new() },
        }
    }
}
