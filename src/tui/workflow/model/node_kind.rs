//! Owner: Interactive TUI subsystem — workflow node-kind taxonomy (U19 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::model::`

use serde::{Deserialize, Serialize};

/// Deployment environment for promotion nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Local,
    Dev,
    Prod,
}

impl Environment {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Dev => "dev",
            Self::Prod => "prod",
        }
    }
}

/// Which side of the merge boundary an agent-review stub sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStage {
    PreMerge,
    PostMerge,
}

impl AgentStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::PreMerge => "pre-merge",
            Self::PostMerge => "post-merge",
        }
    }
}

/// Classification of workflow nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Check,
    Build,
    Lint,
    UnitTest,
    IntegrationTest,
    SecurityGate,
    ReleaseGate,
    VtiPlan,
    Sentinel,
    /// Stubbed agent code-review step (pre- or post-merge).
    AgentReview {
        stage: AgentStage,
    },
    /// Automatic-merge policy node (passes when pre-merge CI + agent review pass).
    AutoMerge,
    /// Immutable artifact build (container image, binary, etc.).
    BuildArtifact,
    /// Promote an artifact into a target environment.
    Promote {
        env: Environment,
    },
    /// Post-deploy monitoring + rollback gate.
    Monitor,
    #[default]
    Custom,
}

impl WorkflowNodeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Build => "build",
            Self::Lint => "lint",
            Self::UnitTest => "unit",
            Self::IntegrationTest => "integration",
            Self::SecurityGate => "security",
            Self::ReleaseGate => "release-gate",
            Self::VtiPlan => "vti-plan",
            Self::Sentinel => "sentinel",
            Self::AgentReview { stage } => match stage {
                AgentStage::PreMerge => "agent-review (pre)",
                AgentStage::PostMerge => "agent-review (post)",
            },
            Self::AutoMerge => "auto-merge",
            Self::BuildArtifact => "build-artifact",
            Self::Promote { env } => match env {
                Environment::Local => "promote local",
                Environment::Dev => "promote dev",
                Environment::Prod => "promote prod",
            },
            Self::Monitor => "monitor",
            Self::Custom => "custom",
        }
    }

    /// Accent glyph rendered on the node card.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::AgentReview { .. } => "🤖",
            Self::AutoMerge => "⇲",
            Self::BuildArtifact => "📦",
            Self::Promote { .. } => "🚀",
            Self::Monitor => "📈",
            _ => "",
        }
    }

    /// True if this node represents a deployment action that can be rolled back.
    pub fn is_rollback_eligible(self) -> bool {
        matches!(
            self,
            Self::Promote {
                env: Environment::Dev | Environment::Prod
            }
        )
    }
}
