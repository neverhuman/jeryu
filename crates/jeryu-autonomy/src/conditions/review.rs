//! Reviewer-driven hard stops: a reviewer issued a hard block, a required
//! reviewer abstained, or a reviewer flagged a prompt-injection-class finding.

use super::HardStop;
use crate::types::{AgentApprovalReceipt, EvidencePack, ReviewDecision};

pub(super) fn cond_reviewer_blocked(
    _p: &EvidencePack,
    receipts: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let blockers: Vec<&AgentApprovalReceipt> = receipts
        .iter()
        .filter(|r| matches!(r.decision, ReviewDecision::Block))
        .collect();
    if blockers.is_empty() {
        return None;
    }
    Some(HardStop {
        name: "reviewer_blocked".into(),
        reason: format!("{} reviewer(s) issued a hard block", blockers.len()),
        details: serde_json::json!({
            "roles": blockers.iter().map(|r| r.role).collect::<Vec<_>>(),
            "agents": blockers.iter().map(|r| r.agent_id.clone()).collect::<Vec<_>>(),
        }),
    })
}

pub(super) fn cond_reviewer_abstained_required(
    _p: &EvidencePack,
    receipts: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let any = receipts
        .iter()
        .any(|r| matches!(r.decision, ReviewDecision::Abstain));
    any.then(|| HardStop {
        name: "reviewer_abstained_required".into(),
        reason: "a required reviewer abstained; fail-closed unless explicit policy override".into(),
        details: serde_json::Value::Null,
    })
}

pub(super) fn cond_prompt_injection_suspected(
    _p: &EvidencePack,
    receipts: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let hits: Vec<&AgentApprovalReceipt> = receipts
        .iter()
        .filter(|r| {
            r.findings
                .iter()
                .any(|f| f.class.starts_with("prompt-injection"))
        })
        .collect();
    (!hits.is_empty()).then(|| HardStop {
        name: "prompt_injection_suspected".into(),
        reason: format!(
            "{} reviewer(s) flagged prompt-injection-class finding",
            hits.len()
        ),
        details: serde_json::Value::Null,
    })
}
