//! Cross-cutting enumerations shared across the canonical objects: risk tiers,
//! reviewer roles, review/gate decisions, and severity.

use serde::{Deserialize, Serialize};

/// One of the 6 risk tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum RiskTier {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
}

impl RiskTier {
    pub fn auto_merge_eligible(self) -> bool {
        matches!(self, RiskTier::R0 | RiskTier::R1 | RiskTier::R2)
    }
    pub fn human_required(self) -> bool {
        matches!(self, RiskTier::R3 | RiskTier::R4 | RiskTier::R5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerRole {
    Security,
    TestIntegrity,
    Runtime,
    Lockfile,
    Judge,
    ReleaseShepherd,
    Nightwatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Pass,
    Concern,
    Block,
    Abstain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    AllowMerge,
    RequireHuman,
    Reject,
}
