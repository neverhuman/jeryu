mod common;

use jeryu_rustjet::{AffectedPlanner, ChangeSet, ImpactReason, PlannerOptions, RunnerClass};

#[test]
fn public_api_change_includes_transitive_dependents() {
    let graph = common::graph();
    let planner = AffectedPlanner::new(&graph);
    let changes = ChangeSet::new(["crates/core/src/lib.rs"]);
    let plan = planner.plan(&changes, &PlannerOptions::default());
    let names = plan.affected_package_names();
    assert!(names.contains("core"));
    assert!(names.contains("api"));
    assert!(names.contains("app"));
    assert!(plan.reasons.contains(&ImpactReason::PublicApiChange));
    assert!(plan.proof_lanes.contains("public-api"));
}

#[test]
fn private_impl_change_includes_direct_reverse_dependents_only() {
    let graph = common::graph();
    let planner = AffectedPlanner::new(&graph);
    let changes = ChangeSet::new(["crates/core/src/internal.rs"]);
    let plan = planner.plan(&changes, &PlannerOptions::default());
    let names = plan.affected_package_names();
    assert!(names.contains("core"));
    assert!(names.contains("api"));
    assert!(!names.contains("app"));
    assert!(
        plan.reasons
            .contains(&ImpactReason::PrivateImplementationChange)
    );
}

#[test]
fn unknown_paths_fail_closed_to_full_workspace() {
    let graph = common::graph();
    let planner = AffectedPlanner::new(&graph);
    let changes = ChangeSet::new(["unexpected/tooling/file.txt"]);
    let plan = planner.plan(&changes, &PlannerOptions::default());
    assert_eq!(plan.affected_package_names().len(), 5);
    assert!(plan.fail_closed);
    assert_eq!(plan.runner_class, RunnerClass::NativeRustClean);
}
