//! Inputs to and output of the judge: the borrowed bundle the judge fuses over
//! ([`JudgeInputs`]) and the resulting verdict plus dropped-receipt log
//! ([`JudgeOutcome`]).

use crate::conditions::HardStop;
use crate::policy_yaml::PolicyBundle;
use crate::types::{AgentApprovalReceipt, EvidencePack, VibeGateVerdict};

pub struct JudgeInputs<'a> {
    pub pack: &'a EvidencePack,
    pub receipts: &'a [AgentApprovalReceipt],
    pub policy: &'a PolicyBundle,
    pub repo: &'a str,
    pub target_branch: &'a str,
    /// Pull request identifier bound to the evidence pack.
    pub pull_request: Option<&'a str>,
    pub author_agent: Option<&'a str>,
    /// Hard stops the orchestrator pre-computed (e.g. `codeowners_not_satisfied`,
    /// `freeze_window_active`, `budget_exceeded`). Merged with registry-computed
    /// hits; ANY hit → Reject (veto > approval).
    pub external_hard_stops: &'a [HardStop],
}

impl<'a> JudgeInputs<'a> {
    /// Convenience constructor with no externally-injected hard stops.
    pub fn new(
        pack: &'a EvidencePack,
        receipts: &'a [AgentApprovalReceipt],
        policy: &'a PolicyBundle,
        repo: &'a str,
        target_branch: &'a str,
    ) -> Self {
        Self {
            pack,
            receipts,
            policy,
            repo,
            target_branch,
            pull_request: None,
            author_agent: None,
            external_hard_stops: &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct JudgeOutcome {
    pub verdict: VibeGateVerdict,
    /// Receipts that failed SHA binding; not included in the verdict.
    pub dropped_receipts: Vec<String>,
}
