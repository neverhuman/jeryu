//! Owner: Interactive TUI subsystem — workflow model tests (U19 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::model::tests`

use super::*;

#[test]
fn status_labels_unique() {
    let all = [
        WorkflowStatus::Waiting,
        WorkflowStatus::Running,
        WorkflowStatus::Ran,
        WorkflowStatus::Error,
        WorkflowStatus::Skipped,
        WorkflowStatus::Cached,
        WorkflowStatus::Blocked,
        WorkflowStatus::Unknown,
    ];
    let labels: Vec<_> = all.iter().map(|s| s.label()).collect();
    let unique: std::collections::HashSet<_> = labels.iter().collect();
    assert_eq!(labels.len(), unique.len());
}

#[test]
fn status_terminal_vs_active() {
    assert!(WorkflowStatus::Ran.is_terminal());
    assert!(!WorkflowStatus::Running.is_terminal());
    assert!(WorkflowStatus::Running.is_active());
}

#[test]
fn summary_from_nodes() {
    let nodes = vec![
        WorkflowNode {
            status: WorkflowStatus::Ran,
            ..Default::default()
        },
        WorkflowNode {
            status: WorkflowStatus::Running,
            ..Default::default()
        },
        WorkflowNode {
            status: WorkflowStatus::Waiting,
            ..Default::default()
        },
        WorkflowNode {
            status: WorkflowStatus::Error,
            ..Default::default()
        },
    ];
    let s = WorkflowSummary::from_nodes(&nodes);
    assert_eq!(s.total, 4);
    assert_eq!(s.passed, 1);
    assert!((s.overall_pct - 50.0).abs() < 0.1);
}

#[test]
fn empty_snapshot_is_demo() {
    let snap = WorkflowSnapshot::empty();
    assert_eq!(snap.source, WorkflowSource::Demo);
    assert!(snap.nodes.is_empty());
}

#[test]
fn node_lookup() {
    let mut snap = WorkflowSnapshot::empty();
    snap.nodes.push(WorkflowNode {
        id: "x".into(),
        ..Default::default()
    });
    assert!(snap.node("x").is_some());
    assert!(snap.node("y").is_none());
}

#[test]
fn canonical_phases_have_unique_slugs() {
    let slugs: std::collections::HashSet<_> =
        CanonicalPhase::ALL.iter().map(|p| p.slug()).collect();
    assert_eq!(slugs.len(), CanonicalPhase::ALL.len());
}

#[test]
fn promote_prod_is_rollback_eligible() {
    let prod = WorkflowNodeKind::Promote {
        env: Environment::Prod,
    };
    let dev = WorkflowNodeKind::Promote {
        env: Environment::Dev,
    };
    let local = WorkflowNodeKind::Promote {
        env: Environment::Local,
    };
    let agent = WorkflowNodeKind::AgentReview {
        stage: AgentStage::PreMerge,
    };
    assert!(prod.is_rollback_eligible());
    assert!(dev.is_rollback_eligible());
    assert!(!local.is_rollback_eligible());
    assert!(!agent.is_rollback_eligible());
}

#[test]
fn pr_cycle_wraps() {
    let mut snap = DeliverySnapshot::empty();
    snap.pull_requests = vec![demo_pr(1), demo_pr(2), demo_pr(3)];

    assert_eq!(snap.selected_pr_idx, 0);
    snap.next_pr();
    assert_eq!(snap.selected_pr_idx, 1);
    snap.next_pr();
    snap.next_pr();
    assert_eq!(snap.selected_pr_idx, 0, "next from last wraps to first");

    snap.prev_pr();
    assert_eq!(snap.selected_pr_idx, 2, "prev from first wraps to last");
}

#[test]
fn pr_select_by_number() {
    let mut snap = DeliverySnapshot::empty();
    snap.pull_requests = vec![demo_pr(101), demo_pr(202), demo_pr(303)];
    assert!(snap.select_by_number(202));
    assert_eq!(snap.selected_pr_idx, 1);
    assert!(!snap.select_by_number(999));
}

#[test]
fn pr_next_on_empty_is_noop() {
    let mut snap = DeliverySnapshot::empty();
    snap.next_pr();
    snap.prev_pr();
    assert_eq!(snap.selected_pr_idx, 0);
}

fn demo_pr(number: u64) -> PullRequestView {
    PullRequestView {
        number,
        title: format!("PR {}", number),
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

#[test]
fn next_pr_matching_skips_non_matching() {
    let mut snap = DeliverySnapshot::empty();
    let mut a = demo_pr(1);
    a.repo_alias = Some("nht".into());
    let mut b = demo_pr(2);
    b.repo_alias = Some("shared".into());
    let mut c = demo_pr(3);
    c.repo_alias = Some("nht".into());
    snap.pull_requests = vec![a, b, c];
    snap.selected_pr_idx = 0;
    snap.next_pr_matching(|pr| pr.repo_alias.as_deref() == Some("nht"));
    assert_eq!(snap.selected_pr_idx, 2);
}

#[test]
fn ensure_selection_matches_jumps_to_first_matching() {
    let mut snap = DeliverySnapshot::empty();
    let mut a = demo_pr(1);
    a.repo_alias = Some("nht".into());
    let mut b = demo_pr(2);
    b.repo_alias = Some("shared".into());
    snap.pull_requests = vec![a, b];
    snap.selected_pr_idx = 0;
    snap.ensure_selection_matches(|pr| pr.repo_alias.as_deref() == Some("shared"));
    assert_eq!(snap.selected_pr_idx, 1);
}

#[test]
fn count_matching_counts_correctly() {
    let mut snap = DeliverySnapshot::empty();
    let mut a = demo_pr(1);
    a.repo_alias = Some("nht".into());
    let mut b = demo_pr(2);
    b.repo_alias = Some("shared".into());
    let mut c = demo_pr(3);
    c.repo_alias = Some("nht".into());
    snap.pull_requests = vec![a, b, c];
    assert_eq!(
        snap.count_matching(|pr| pr.repo_alias.as_deref() == Some("nht")),
        2
    );
    assert_eq!(snap.count_matching(|_| true), 3);
}
