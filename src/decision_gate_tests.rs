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
