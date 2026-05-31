//! CI-status hard stops: a required CI / required-check lane that is **absent**
//! from the pack's `ci_status` (`missing_required_ci_check`) or **present but
//! not green** (`failed_required_ci_check`).
//!
//! These two conditions close the pre-merge CI gate: today the FULL-AUTO loader
//! makes `R0..=R4` AllowMerge-eligible but never checks that the PR's required
//! lanes are green, so an R0-R4 PR could auto-merge with RED or MISSING checks.
//!
//! Unlike the pack-local detectors, the *required lane set* is a policy concern
//! (`approvals.required_ci_lanes`), not pack data, so the registry entries are
//! externally-supplied no-ops (they keep the registry total and let policy
//! reference the names). The judge — which holds both the pack and the policy —
//! computes the real hits via [`ci_hard_stops`] and merges them into the
//! hard-stop walk, exactly as it does for `external_hard_stops`. Because the
//! merge happens *before* the quorum/decision step and ANY hit forces `Reject`,
//! the veto-beats-approval invariant holds even under a full-auto profile.

use super::HardStop;
use crate::types::{CiCheck, CiConclusion, EvidencePack};

/// Registry name for "a required CI lane is absent from `ci_status`".
pub(super) const MISSING_REQUIRED_CI_CHECK: &str = "missing_required_ci_check";
/// Registry name for "a required CI lane is present but its conclusion is not
/// `Success`".
pub(super) const FAILED_REQUIRED_CI_CHECK: &str = "failed_required_ci_check";

/// Look up the reported conclusion for `lane` in the pack's `ci_status`, if any.
/// Lane names are matched exactly (case-sensitive), matching forge check-run
/// names. The first entry wins if a lane is reported more than once.
fn conclusion_for<'a>(ci_status: &'a [CiCheck], lane: &str) -> Option<&'a CiCheck> {
    ci_status.iter().find(|c| c.name == lane)
}

/// Compute the CI hard-stops for `pack` against the policy-declared
/// `required_lanes`.
///
/// For every required lane:
///   * absent from `pack.ci_status` → `missing_required_ci_check`;
///   * present with a non-`Success` conclusion → `failed_required_ci_check`.
///
/// An empty `required_lanes` yields no hits (no CI gate — back-compat). The
/// returned hits preserve `required_lanes` order so the verdict is
/// deterministic.
pub fn ci_hard_stops(pack: &EvidencePack, required_lanes: &[String]) -> Vec<HardStop> {
    let mut out = Vec::new();
    for lane in required_lanes {
        match conclusion_for(&pack.ci_status, lane) {
            None => out.push(HardStop {
                name: MISSING_REQUIRED_CI_CHECK.into(),
                reason: format!("required CI lane '{lane}' is absent from ci_status; fail-closed"),
                details: serde_json::json!({
                    "lane": lane,
                    "conclusion": CiConclusion::Missing,
                }),
            }),
            Some(check) if !check.conclusion.is_green() => out.push(HardStop {
                name: FAILED_REQUIRED_CI_CHECK.into(),
                reason: format!(
                    "required CI lane '{lane}' is not green (conclusion is not success)"
                ),
                details: serde_json::json!({
                    "lane": lane,
                    "conclusion": check.conclusion,
                }),
            }),
            Some(_) => {}
        }
    }
    out
}

/// Registry placeholder for `missing_required_ci_check`. The real evaluation
/// needs the policy's `required_ci_lanes` (not in the pack), so the judge
/// injects the computed hit via [`ci_hard_stops`]; locally this is a no-op so
/// the registry stays total and policy may reference the name.
pub(super) fn cond_missing_required_ci_check(
    _p: &EvidencePack,
    _r: &[crate::types::AgentApprovalReceipt],
) -> Option<HardStop> {
    None
}

/// Registry placeholder for `failed_required_ci_check`. See
/// [`cond_missing_required_ci_check`] — the judge injects the real hit.
pub(super) fn cond_failed_required_ci_check(
    _p: &EvidencePack,
    _r: &[crate::types::AgentApprovalReceipt],
) -> Option<HardStop> {
    None
}
