//! Admission record repository boundary.

use crate::admission::{AdmissionEvaluation, AdmissionVerdict, evaluate_pre_receive_line};

pub async fn record_decision_for_hook(raw_input: &str, evaluation: &AdmissionEvaluation) -> bool {
    crate::state::record_admission_decision_for_hook(raw_input, evaluation).await
}

pub async fn evaluate_line_with_records(
    line: &str,
    enforce_agent_grant: bool,
    store: &crate::state::Db,
) -> AdmissionEvaluation {
    let mut evaluation = evaluate_pre_receive_line(line, enforce_agent_grant);
    if evaluation.actor_kind != "agent" {
        return evaluation;
    }

    let Some(ref_name) = evaluation.ref_name.as_deref() else {
        return evaluation;
    };
    let new_sha = evaluation.new_sha.as_deref();
    match store
        .active_capability_grant_for_ref(ref_name, new_sha)
        .await
    {
        Ok(Some(grant)) => {
            evaluation.verdict = AdmissionVerdict::Allow;
            evaluation.grant_id = Some(grant.grant_id);
            evaluation
                .reasons
                .retain(|reason| reason != "agent intent grant verification pending");
            evaluation
                .reasons
                .push("agent write matched active capability grant".to_string());
        }
        Ok(None) => {}
        Err(err) => {
            if enforce_agent_grant {
                evaluation.verdict = AdmissionVerdict::Deny;
            }
            evaluation
                .reasons
                .push(format!("admission record lookup failed: {err}"));
        }
    }
    evaluation
}
