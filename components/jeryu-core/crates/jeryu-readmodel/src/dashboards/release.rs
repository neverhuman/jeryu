//! Release dashboard contract — release-candidate status / SBOM / promotion.
//!
//! Pure data; freshness carried alongside; default = "empty/unavailable". Each
//! item is a release candidate: its gate posture, the SBOM coverage backing it,
//! and the promotion stage it has reached.

use serde::{Deserialize, Serialize};

use crate::entity::HealthLevel;
use crate::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ReleaseSnapshot {
    pub items: Vec<ReleaseItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<ReleaseSummary>,
}

impl ReleaseSnapshot {
    /// Count of candidates blocked by a failing gate.
    pub fn blocked(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.gate == ReleaseGate::Blocked)
            .count() as u32
    }
}

/// One release candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseItem {
    pub release_id: String,
    pub label: String,
    pub candidate_sha: String,
    /// Gate posture for this candidate.
    pub gate: ReleaseGate,
    /// Promotion stage the candidate has reached.
    pub stage: PromotionStage,
    /// SBOM coverage status backing the candidate.
    pub sbom: SbomStatus,
    pub rollback_target: Option<String>,
}

impl ReleaseItem {
    pub fn new(release_id: impl Into<String>, candidate_sha: impl Into<String>) -> Self {
        let release_id = release_id.into();
        Self {
            label: release_id.clone(),
            release_id,
            candidate_sha: candidate_sha.into(),
            gate: ReleaseGate::Pending,
            stage: PromotionStage::Candidate,
            sbom: SbomStatus::Missing,
            rollback_target: None,
        }
    }
}

impl Default for ReleaseItem {
    fn default() -> Self {
        Self::new("unknown", "0000000")
    }
}

/// Release gate posture.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseGate {
    /// All gates satisfied; candidate may promote.
    Ready,
    /// Gates still evaluating.
    Pending,
    /// A gate failed; promotion is blocked.
    Blocked,
}

impl ReleaseGate {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Pending => "pending",
            Self::Blocked => "BLOCKED",
        }
    }
}

/// Where the candidate sits in the promotion pipeline.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromotionStage {
    Candidate,
    Canary,
    Production,
    RolledBack,
}

impl PromotionStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Canary => "canary",
            Self::Production => "production",
            Self::RolledBack => "rolled_back",
        }
    }
}

/// SBOM coverage backing a release candidate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SbomStatus {
    /// SBOM present and signed/verified.
    Verified,
    /// SBOM present but unverified.
    Present,
    /// No SBOM attached.
    Missing,
}

impl SbomStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Present => "present",
            Self::Missing => "MISSING",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseSummary {
    pub candidate_ready: bool,
    pub canary_passing: bool,
    pub production_health: HealthLevel,
    pub blocked_count: u32,
}

impl Default for ReleaseSummary {
    fn default() -> Self {
        Self {
            candidate_ready: false,
            canary_passing: false,
            production_health: HealthLevel::Unknown,
            blocked_count: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = ReleaseSnapshot::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
        assert_eq!(d.blocked(), 0);
    }

    #[test]
    fn enums_round_trip() {
        assert_eq!(
            serde_json::to_string(&ReleaseGate::Blocked).unwrap(),
            "\"blocked\""
        );
        assert_eq!(
            serde_json::to_string(&PromotionStage::Canary).unwrap(),
            "\"canary\""
        );
        assert_eq!(
            serde_json::to_string(&SbomStatus::Verified).unwrap(),
            "\"verified\""
        );
    }

    #[test]
    fn blocked_counts_blocked_candidates() {
        let mut blocked = ReleaseItem::new("rel-1", "abc1234");
        blocked.gate = ReleaseGate::Blocked;
        let d = ReleaseSnapshot {
            items: vec![blocked, ReleaseItem::new("rel-2", "def5678")],
            freshness: None,
            summary: None,
        };
        assert_eq!(d.blocked(), 1);
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = ReleaseSnapshot {
            items: vec![ReleaseItem::new("rel-1", "abc1234")],
            freshness: None,
            summary: Some(ReleaseSummary::default()),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: ReleaseSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
