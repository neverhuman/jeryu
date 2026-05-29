use super::{
    delivery,
    model::{
        CanonicalPhase, DeliverySnapshot, Environment, PrStatus, WorkflowNodeKind,
        WorkflowSnapshot, WorkflowStatus, WorkflowSummary,
    },
    select_workflow_lens_input,
};

#[test]
fn model_facade_preserves_status_and_snapshot_behavior() {
    assert_eq!(WorkflowStatus::Ran.label(), "RAN");
    assert!(WorkflowStatus::Ran.is_terminal());
    assert!(WorkflowStatus::Running.is_active());

    let snapshot = WorkflowSnapshot::empty();
    assert_eq!(snapshot.title, "No active workflow");
    assert_eq!(snapshot.nodes.len(), 0);
}

#[test]
fn model_facade_preserves_delivery_selection_behavior() {
    let mut delivery = DeliverySnapshot::empty();
    delivery.pull_requests = vec![demo_pr(11), demo_pr(22)];

    assert_eq!(delivery.selected().map(|pr| pr.number), Some(11));
    delivery.next_pr();
    assert_eq!(delivery.selected().map(|pr| pr.number), Some(22));
    assert!(delivery.select_by_number(11));
    assert_eq!(delivery.selected().map(|pr| pr.number), Some(11));
}

#[test]
fn delivery_facade_preserves_demo_pipeline_story() {
    let delivery = delivery::build_demo_delivery();
    assert_eq!(delivery.pull_requests.len(), 5);
    assert!(
        delivery
            .pull_requests
            .iter()
            .any(|pr| pr.status == PrStatus::Blocked),
        "demo should retain a blocked PR"
    );
    assert!(
        delivery
            .pull_requests
            .iter()
            .any(|pr| pr.phase == CanonicalPhase::PromoteDev),
        "demo should retain a canary/dev promotion story"
    );
    assert!(delivery.fleet_summary.open_prs >= 5);
}

#[test]
fn workflow_selector_exposes_selected_snapshot() {
    let delivery = delivery::build_demo_delivery();
    let input = select_workflow_lens_input(&delivery);

    assert!(input.has_prs());
    assert!(input.selected_title().is_some());
    assert!(input.selected_node_count() > 0);
}

#[test]
fn node_kind_facade_preserves_rollback_semantics() {
    let prod = WorkflowNodeKind::Promote {
        env: Environment::Prod,
    };
    let local = WorkflowNodeKind::Promote {
        env: Environment::Local,
    };

    assert!(prod.is_rollback_eligible());
    assert!(!local.is_rollback_eligible());
}

fn demo_pr(number: u64) -> crate::tui::lenses::workflow::model::PullRequestView {
    crate::tui::lenses::workflow::model::PullRequestView {
        number,
        title: format!("PR {number}"),
        author: "alice".into(),
        head_sha: "deadbeef".into(),
        status: PrStatus::Open,
        phase: CanonicalPhase::PreMergeCI,
        mergeable: true,
        ci_summary: WorkflowSummary::default(),
        age_secs: 60,
        draft: false,
        labels: vec![],
        current_node_id: None,
        snapshot: WorkflowSnapshot::empty(),
        repo_alias: None,
        repo_slug: None,
    }
}
