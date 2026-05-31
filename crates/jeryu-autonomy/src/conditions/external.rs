//! Externally-supplied conditions.
//!
//! These depend on context the registry doesn't have at fusion time
//! (SHA/policy drift, freeze windows, budgets, codeowners, release-artifact
//! integrity, …). The judge / orchestrator pre-evaluates them and injects them
//! by name via `external_hard_stops`; locally they are a total no-op.

use super::HardStop;
use crate::types::{AgentApprovalReceipt, EvidencePack};

/// Condition that depends on context the registry doesn't have at fusion time.
/// The judge / orchestrator pre-evaluates these and injects them by name.
pub(super) fn cond_externally_supplied(
    _p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    None
}
