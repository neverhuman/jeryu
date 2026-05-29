use super::{
    agent_review::agent_review_receipt_status, auto_merge::auto_merge_gate_status,
    build_demo_delivery,
};
use crate::tui::lenses::workflow::model::{CanonicalPhase, PrStatus, WorkflowStatus};

#[test]
fn demo_delivery_renders_all_5_prs() {
    let snapshot = build_demo_delivery();
    assert_eq!(snapshot.pull_requests.len(), 5);
    let mut numbers: Vec<u64> = snapshot.pull_requests.iter().map(|pr| pr.number).collect();
    numbers.sort();
    numbers.dedup();
    assert_eq!(numbers.len(), 5);
}

#[test]
fn pr_with_failed_test_is_blocked() {
    let snapshot = build_demo_delivery();
    let pr = snapshot
        .pull_requests
        .iter()
        .find(|pr| pr.number == 1842)
        .unwrap();
    assert_eq!(pr.status, PrStatus::Blocked);
}

#[test]
fn merged_pr_in_canary_is_at_promote_dev() {
    let snapshot = build_demo_delivery();
    let pr = snapshot
        .pull_requests
        .iter()
        .find(|pr| pr.number == 1835)
        .unwrap();
    assert_eq!(pr.status, PrStatus::Merged);
    assert_eq!(pr.phase, CanonicalPhase::PromoteDev);
}

#[test]
fn fleet_summary_counts_open_and_blocked() {
    let snapshot = build_demo_delivery();
    let fleet = &snapshot.fleet_summary;
    assert_eq!(fleet.open_prs, 5);
    assert!(fleet.blocked >= 1);
    assert!(fleet.canary_in_flight);
}

#[test]
fn agent_review_is_receipt_backed() {
    assert_eq!(
        agent_review_receipt_status(WorkflowStatus::Ran, &["agent-review:pass".into()]),
        WorkflowStatus::Ran
    );
    assert_eq!(
        agent_review_receipt_status(WorkflowStatus::Ran, &[]),
        WorkflowStatus::Waiting
    );
}

#[test]
fn auto_merge_passes_when_all_green() {
    assert_eq!(
        auto_merge_gate_status(WorkflowStatus::Ran, WorkflowStatus::Ran),
        WorkflowStatus::Ran
    );
    assert_eq!(
        auto_merge_gate_status(WorkflowStatus::Error, WorkflowStatus::Ran),
        WorkflowStatus::Blocked
    );
}

#[test]
fn canonical_pipeline_has_all_phases_for_merged_pr() {
    let snapshot = build_demo_delivery();
    let pr = snapshot
        .pull_requests
        .iter()
        .find(|pr| pr.number == 1835)
        .unwrap();
    let slugs: std::collections::HashSet<_> = pr
        .snapshot
        .phases
        .iter()
        .map(|phase| phase.id.as_str())
        .collect();
    for canonical in CanonicalPhase::ALL {
        assert!(
            slugs.contains(canonical.slug()),
            "missing canonical phase {}",
            canonical.slug()
        );
    }
}
