use super::*;

pub fn evaluate_risk_gate(
    trust_tier: TrustTier,
    successful_jobs: usize,
    pending_jobs: usize,
    failed_jobs: usize,
    policy: &RequiredEvidencePolicy,
) -> RiskEvaluation {
    if policy.require_no_recent_failures && failed_jobs > 0 {
        return RiskEvaluation {
            decision: RiskGateDecision::Deny,
            reason: "failed jobs are still present for the merge ref".to_string(),
            trust_tier,
        };
    }

    if policy.require_no_pending_jobs && pending_jobs > 0 {
        return RiskEvaluation {
            decision: RiskGateDecision::Escalate,
            reason: "pending or running jobs still exist for the merge ref".to_string(),
            trust_tier,
        };
    }

    if policy.require_successful_jobs && successful_jobs == 0 {
        return RiskEvaluation {
            decision: RiskGateDecision::Deny,
            reason: "no successful validation jobs were found for the merge ref".to_string(),
            trust_tier,
        };
    }

    match trust_tier {
        TrustTier::Untrusted => RiskEvaluation {
            decision: RiskGateDecision::Escalate,
            reason: "untrusted tier requires a human escalation before merge".to_string(),
            trust_tier,
        },
        TrustTier::Trusted | TrustTier::Privileged => RiskEvaluation {
            decision: RiskGateDecision::Allow,
            reason: "required evidence policy satisfied".to_string(),
            trust_tier,
        },
    }
}

/// Evaluates validation, selector, cache, and trust evidence into a merge proof.
pub fn evaluate_merge_gate(
    input: MergeGateInput,
    policy: &RequiredEvidencePolicy,
) -> MergeGateProof {
    let mut blockers = Vec::new();
    if input.selector_misses > 0 {
        blockers.push(format!(
            "test selector has {} unresolved miss(es)",
            input.selector_misses
        ));
    }
    if input.cache_taints > 0 {
        blockers.push(format!("{} active cache taint(s)", input.cache_taints));
    }
    if policy.require_vti_receipt {
        match &input.vti_receipt {
            Some(receipt) => {
                if !receipt.skipped_tests_explained && receipt.mode != "full" {
                    blockers.push("VTI receipt does not explain skipped validation".to_string());
                }
                if let (Some(receipt_sha), Some(head_sha)) = (&receipt.head_sha, &input.head_sha)
                    && receipt_sha != head_sha
                {
                    blockers.push(format!(
                        "VTI receipt head SHA {} does not match merge head {}",
                        receipt_sha, head_sha
                    ));
                }
            }
            None => blockers.push("missing VTI validation receipt".to_string()),
        }
    }

    let risk = evaluate_risk_gate(
        input.trust_tier.clone(),
        input.successful_jobs,
        input.pending_jobs,
        input.failed_jobs,
        policy,
    );
    if risk.decision != RiskGateDecision::Allow {
        blockers.push(risk.reason.clone());
    }

    let decision = if blockers.is_empty() {
        RiskGateDecision::Allow
    } else if risk.decision == RiskGateDecision::Escalate {
        RiskGateDecision::Escalate
    } else {
        RiskGateDecision::Deny
    };

    MergeGateProof {
        decision,
        project_id: input.project_id,
        mr_iid: input.mr_iid,
        source_branch: input.source_branch,
        target_branch: input.target_branch,
        head_sha: input.head_sha,
        blockers,
        successful_jobs: input.successful_jobs,
        pending_jobs: input.pending_jobs,
        failed_jobs: input.failed_jobs,
        selector_misses: input.selector_misses,
        cache_taints: input.cache_taints,
        vti_receipt: input.vti_receipt,
        trust_tier: input.trust_tier,
        policy_version: "merge-gate-v3.01".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn capsule(kind: &str, log: &str, exit_code: i32) -> FailureCapsule {
        FailureCapsule {
            job_id: 1,
            pipeline_id: Some(2),
            project_id: 3,
            stage: "test".into(),
            exit_code,
            commit_sha: "abc".into(),
            ref_name: "main".into(),
            working_directory: "/builds/project".into(),
            log_snippet: log.into(),
            repro_script: "cargo test".into(),
            environment: HashMap::new(),
            failure_kind: kind.into(),
            summary: "summary".into(),
            superseded_by_sha: None,
            requeued_from_job_id: None,
        }
    }

    fn ok_vti_receipt() -> VtiReceiptSummary {
        VtiReceiptSummary {
            receipt_id: "vti-ok".into(),
            mode: "full".into(),
            head_sha: Some("abc".into()),
            skipped_tests_explained: true,
            widened_to_full: false,
        }
    }

    fn merge_gate_input_fixture() -> MergeGateInput {
        MergeGateInput {
            project_id: 1,
            mr_iid: 2,
            source_branch: "agent/task".into(),
            target_branch: "main".into(),
            head_sha: Some("abc".into()),
            successful_jobs: 3,
            pending_jobs: 0,
            failed_jobs: 0,
            selector_misses: 0,
            cache_taints: 0,
            vti_receipt: Some(ok_vti_receipt()),
            trust_tier: TrustTier::Trusted,
        }
    }

    #[test]
    fn classifies_transient_failures() {
        let result = classify_failure(&capsule("timeout", "network connection reset", 1));
        assert_eq!(result, FailureClassification::Transient);
    }

    #[test]
    fn recommends_failure_response_for_transient_failures() {
        let result = failure_response_for(&capsule("timeout", "timed out", 124));
        assert_eq!(result, RetryDecision::RetryOnce);
    }

    #[test]
    fn risk_gate_denies_failed_refs() {
        let result = evaluate_risk_gate(
            TrustTier::Trusted,
            1,
            0,
            1,
            &RequiredEvidencePolicy::default(),
        );
        assert_eq!(result.decision, RiskGateDecision::Deny);
    }

    #[test]
    fn merge_gate_allows_clean_trusted_ref() {
        let proof = evaluate_merge_gate(
            merge_gate_input_fixture(),
            &RequiredEvidencePolicy::default(),
        );
        assert_eq!(proof.decision, RiskGateDecision::Allow);
        assert!(proof.blockers.is_empty());
    }

    #[test]
    fn merge_gate_denies_selector_miss_and_taint() {
        let mut input = merge_gate_input_fixture();
        input.selector_misses = 1;
        input.cache_taints = 2;
        let proof = evaluate_merge_gate(input, &RequiredEvidencePolicy::default());
        assert_eq!(proof.decision, RiskGateDecision::Deny);
        assert_eq!(proof.blockers.len(), 2);
    }

    #[test]
    fn merge_gate_denies_missing_vti_receipt() {
        let mut input = merge_gate_input_fixture();
        input.vti_receipt = None;
        let proof = evaluate_merge_gate(input, &RequiredEvidencePolicy::default());
        assert_eq!(proof.decision, RiskGateDecision::Deny);
        assert!(
            proof
                .blockers
                .contains(&"missing VTI validation receipt".to_string())
        );
    }
}
