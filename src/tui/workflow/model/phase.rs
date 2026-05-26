//! Owner: Interactive TUI subsystem — workflow phase types (U19 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::model::`

use serde::{Deserialize, Serialize};

/// A horizontal row of parallel nodes at the same dependency depth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    pub id: String,
    pub title: String,
    pub depth: u32,
    pub node_ids: Vec<String>,
}

/// Canonical phase names for the end-to-end Delivery view.
///
/// These map a PR's progress through the developer pipeline so the TUI can
/// render a consistent phase rail / minimap independent of how the underlying
/// CI happens to group jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalPhase {
    PreMergeCI,
    AgentReviewPreMerge,
    AutoMerge,
    PostMergeCI,
    AgentReviewPostMerge,
    BuildArtifact,
    PromoteLocal,
    PromoteDev,
    PromoteProd,
    MonitorRollback,
}

impl CanonicalPhase {
    pub const ALL: [CanonicalPhase; 10] = [
        Self::PreMergeCI,
        Self::AgentReviewPreMerge,
        Self::AutoMerge,
        Self::PostMergeCI,
        Self::AgentReviewPostMerge,
        Self::BuildArtifact,
        Self::PromoteLocal,
        Self::PromoteDev,
        Self::PromoteProd,
        Self::MonitorRollback,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::PreMergeCI => "Pre-merge CI",
            Self::AgentReviewPreMerge => "Agent review (pre)",
            Self::AutoMerge => "Auto-merge",
            Self::PostMergeCI => "Post-merge CI",
            Self::AgentReviewPostMerge => "Agent review (post)",
            Self::BuildArtifact => "Build artifact",
            Self::PromoteLocal => "Promote → local",
            Self::PromoteDev => "Promote → dev",
            Self::PromoteProd => "Promote → prod",
            Self::MonitorRollback => "Monitor / rollback",
        }
    }

    /// Short label used by the left-side phase rail (≤ 7 chars).
    pub fn short(self) -> &'static str {
        match self {
            Self::PreMergeCI => "PreCI",
            Self::AgentReviewPreMerge => "Agent▲",
            Self::AutoMerge => "Merge",
            Self::PostMergeCI => "PostCI",
            Self::AgentReviewPostMerge => "Agent▼",
            Self::BuildArtifact => "Build",
            Self::PromoteLocal => "Local",
            Self::PromoteDev => "Dev",
            Self::PromoteProd => "Prod",
            Self::MonitorRollback => "Watch",
        }
    }

    /// Stable id string for use in phase/node keys.
    pub fn slug(self) -> &'static str {
        match self {
            Self::PreMergeCI => "pre-merge-ci",
            Self::AgentReviewPreMerge => "agent-review-pre",
            Self::AutoMerge => "auto-merge",
            Self::PostMergeCI => "post-merge-ci",
            Self::AgentReviewPostMerge => "agent-review-post",
            Self::BuildArtifact => "build-artifact",
            Self::PromoteLocal => "promote-local",
            Self::PromoteDev => "promote-dev",
            Self::PromoteProd => "promote-prod",
            Self::MonitorRollback => "monitor",
        }
    }
}
