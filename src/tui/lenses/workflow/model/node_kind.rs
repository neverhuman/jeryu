//! Workflow node-kind model.

use serde::{Deserialize, Serialize};

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
    AgentReview {
        stage: AgentStage,
    },
    AutoMerge,
    BuildArtifact,
    Promote {
        env: Environment,
    },
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

    pub fn is_rollback_eligible(self) -> bool {
        matches!(
            self,
            Self::Promote {
                env: Environment::Dev | Environment::Prod
            }
        )
    }
}
