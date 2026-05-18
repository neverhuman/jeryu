//! Owner: TUI Control-Plane API — view-specific snapshot builders
//! Proof: `cargo nextest run -p jeryu -- api::snapshot`
//! Invariants: Snapshot builders produce typed view models from raw control-plane state.

use serde::{Deserialize, Serialize};

/// VTI (Validated Test Intelligence) status for a job or test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VtiStatus {
    /// Test was selected by the VTI engine.
    Selected { reason: String, confidence: f64 },
    /// Test was skipped by the VTI engine.
    Skipped { reason: String, confidence: f64 },
    /// Test was accelerated (cache hit or reuse). 🔥
    Accelerated {
        reason: String,
        time_saved_secs: i64,
    },
    /// Full suite run (no VTI filtering applied).
    FullSuite,
}

impl VtiStatus {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Selected { .. } => "[SEL]",
            Self::Skipped { .. } => "[SKIP]",
            Self::Accelerated { .. } => "[🔥 VTI]",
            Self::FullSuite => "[FULL]",
        }
    }
    pub fn is_accelerated(&self) -> bool {
        matches!(self, Self::Accelerated { .. })
    }
    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped { .. })
    }
}

/// Cache verdict for a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheVerdict {
    Hit { trust: CacheTrust },
    Miss,
    Tainted { reason: String },
    Denied { reason: String },
}

impl CacheVerdict {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Hit { .. } => "[HIT]",
            Self::Miss => "[MISS]",
            Self::Tainted { .. } => "[TAINT]",
            Self::Denied { .. } => "[DENIED]",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheTrust {
    Trusted,
    Untrusted,
    Verified,
}

/// Edge classification for the flow graph.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Real GitLab `needs:` dependency ──▶
    GitlabNeeds,
    /// Artifact dependency ══▶
    ArtifactDep,
    /// Inferred stage ordering - -▶
    StageOrder,
    /// VTI skip edge ··▶
    VtiSkipped,
    /// Blocked dependency ✗─▶
    Blocked,
    /// Downstream child pipeline ──▷
    ChildPipeline,
}

impl EdgeKind {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::GitlabNeeds => "──▶",
            Self::ArtifactDep => "══▶",
            Self::StageOrder => "- -▶",
            Self::VtiSkipped => "··▶",
            Self::Blocked => "✗─▶",
            Self::ChildPipeline => "──▷",
        }
    }
}

/// Test plan summary for the Tests/VTI tab.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestPlanView {
    pub ref_name: String,
    pub base_sha: String,
    pub head_sha: String,
    pub changed_files: Vec<String>,
    pub selected_tests: Vec<TestSelection>,
    pub skipped_tests: Vec<TestSelection>,
    pub accelerated_tests: Vec<TestSelection>,
    pub confidence: f64,
    pub selector_misses_24h: u32,
    pub selector_misses_7d: u32,
    pub unknown_paths: u32,
    pub global_invalidators_touched: bool,
    pub decision: ValidationDecision,
    pub total_time_saved_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSelection {
    pub test_name: String,
    pub reason: String,
    pub confidence: f64,
    pub vti_status: VtiStatus,
    pub impacted_by: Vec<String>,
    pub estimated_duration_secs: Option<u64>,
    pub flake_probability: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationDecision {
    #[default]
    Unknown,
    Valid,
    Invalid,
    Escalate,
}

impl ValidationDecision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::Valid => "VALID",
            Self::Invalid => "INVALID",
            Self::Escalate => "ESCALATE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vti_badges_are_distinct() {
        let sel = VtiStatus::Selected {
            reason: "dep".into(),
            confidence: 0.9,
        };
        let skip = VtiStatus::Skipped {
            reason: "no path".into(),
            confidence: 0.95,
        };
        let acc = VtiStatus::Accelerated {
            reason: "cache".into(),
            time_saved_secs: 42,
        };
        assert_ne!(sel.badge(), skip.badge());
        assert_ne!(skip.badge(), acc.badge());
        assert!(acc.is_accelerated());
        assert!(skip.is_skipped());
    }

    #[test]
    fn edge_kind_glyphs() {
        assert_eq!(EdgeKind::GitlabNeeds.glyph(), "──▶");
        assert_eq!(EdgeKind::VtiSkipped.glyph(), "··▶");
    }

    #[test]
    fn default_test_plan_is_unknown() {
        let p = TestPlanView::default();
        assert_eq!(p.decision, ValidationDecision::Unknown);
        assert_eq!(p.confidence, 0.0);
    }
}
