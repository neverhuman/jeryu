//! Shared Jain ecosystem compliance contract.

use anyhow::{Result, bail};
use serde::Deserialize;

pub(super) const MINIMUM_JANKURAI_SCORE: u64 = 85;
pub(super) const MAX_AUTHORED_LINES: u64 = 500;
pub(super) const TARGET_AUTHORED_LINES: u64 = 300;
pub(super) const MAX_RAW_GIT_BLOB_BYTES: u64 = 1_048_576;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum ComplianceState {
    Inventory,
    Enforced,
}

impl ComplianceState {
    pub(super) fn validate_transition(previous: Self, proposed: Self) -> Result<()> {
        if previous == Self::Enforced && proposed == Self::Inventory {
            bail!("compliance_state cannot transition from enforced back to inventory");
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ComplianceContract {
    pub(super) minimum_jankurai_score: u64,
    pub(super) max_authored_lines: u64,
    pub(super) target_authored_lines: u64,
    pub(super) max_raw_git_blob_bytes: u64,
    pub(super) max_caps: u64,
    pub(super) max_hard_findings: u64,
    pub(super) max_soft_findings: u64,
    pub(super) max_disabled_rules: u64,
    pub(super) allowed_score_drop: u64,
}

pub(super) fn validate_compliance(contract: &ComplianceContract) -> Result<()> {
    let expected = (
        MINIMUM_JANKURAI_SCORE,
        MAX_AUTHORED_LINES,
        TARGET_AUTHORED_LINES,
        MAX_RAW_GIT_BLOB_BYTES,
        0,
        0,
        0,
        0,
        0,
    );
    let actual = (
        contract.minimum_jankurai_score,
        contract.max_authored_lines,
        contract.target_authored_lines,
        contract.max_raw_git_blob_bytes,
        contract.max_caps,
        contract.max_hard_findings,
        contract.max_soft_findings,
        contract.max_disabled_rules,
        contract.allowed_score_drop,
    );
    if actual != expected {
        bail!(
            "compliance contract must require score>=85, authored<=500 (target 300), raw Git blobs<=1048576 bytes, and zero caps/findings/disabled rules/score drop"
        );
    }
    Ok(())
}
