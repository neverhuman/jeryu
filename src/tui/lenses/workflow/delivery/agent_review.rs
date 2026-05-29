//! Agent-review delivery status helpers.

use crate::tui::lenses::workflow::model::{
    AgentCallDetail, AgentFindingBrief, AgentStage, WorkflowStatus,
};

pub(super) fn agent_review_receipt_status(
    upstream: WorkflowStatus,
    labels: &[String],
) -> WorkflowStatus {
    match upstream {
        WorkflowStatus::Ran | WorkflowStatus::Cached => receipt_status_from_labels(labels),
        WorkflowStatus::Error => WorkflowStatus::Blocked,
        WorkflowStatus::Running | WorkflowStatus::Waiting => WorkflowStatus::Waiting,
        WorkflowStatus::Skipped => WorkflowStatus::Skipped,
        WorkflowStatus::Blocked => WorkflowStatus::Blocked,
        WorkflowStatus::Unknown => WorkflowStatus::Unknown,
    }
}

fn receipt_status_from_labels(labels: &[String]) -> WorkflowStatus {
    for label in labels {
        let normalized = label.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "agent-review:pass" | "agent-review=pass" | "jeryu:agent-review=pass" => {
                return WorkflowStatus::Ran;
            }
            "agent-review:block" | "agent-review=block" | "jeryu:agent-review=block" => {
                return WorkflowStatus::Blocked;
            }
            "agent-review:running" | "agent-review=running" | "jeryu:agent-review=running" => {
                return WorkflowStatus::Running;
            }
            _ => {}
        }
    }
    WorkflowStatus::Waiting
}

pub(super) fn agent_review_reason(status: WorkflowStatus) -> String {
    match status {
        WorkflowStatus::Ran => "Signed agent-review receipt accepted.".into(),
        WorkflowStatus::Blocked | WorkflowStatus::Error => {
            "Signed agent-review receipt blocks the MR.".into()
        }
        WorkflowStatus::Running => "Agent-review session is running.".into(),
        _ => "Waiting for signed agent-review receipt.".into(),
    }
}

pub(super) fn demo_agent_call(
    status: WorkflowStatus,
    stage: AgentStage,
) -> Option<AgentCallDetail> {
    let agent_id = match stage {
        AgentStage::PreMerge => "reviewer/pre-merge",
        AgentStage::PostMerge => "reviewer/post-merge",
    };
    let (decision, reason, findings) = match status {
        WorkflowStatus::Ran => (
            Some("pass".to_string()),
            Some("No blocking issues. All proofs reproduce.".to_string()),
            Vec::new(),
        ),
        WorkflowStatus::Blocked | WorkflowStatus::Error => (
            Some("block".to_string()),
            Some(
                "Type error in src/pages/runs.tsx:42 — schema mismatch with pagination cursor."
                    .to_string(),
            ),
            vec![AgentFindingBrief {
                severity: "high".into(),
                class: "type-error".into(),
                file: Some("src/pages/runs.tsx:42".into()),
            }],
        ),
        WorkflowStatus::Running => (
            None,
            Some("Reviewing diff; structured findings will populate on completion.".to_string()),
            Vec::new(),
        ),
        _ => return None,
    };
    Some(AgentCallDetail {
        model: Some("claude-opus-4-7".into()),
        provider: Some("anthropic".into()),
        agent_id: Some(agent_id.into()),
        decision,
        reason,
        raw_response_sha: Some("0123abcdef456789".into()),
        findings,
        decision_json: None,
    })
}
