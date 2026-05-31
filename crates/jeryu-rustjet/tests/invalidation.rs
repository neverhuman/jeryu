mod common;

use jeryu_rustjet::{AffectedPlanner, ChangeSet, ImpactReason, PlannerOptions};

#[test]
fn cargo_lock_change_invalidates_entire_workspace() {
    let graph = common::graph();
    let plan = AffectedPlanner::new(&graph)
        .plan(&ChangeSet::new(["Cargo.lock"]), &PlannerOptions::default());
    assert_eq!(plan.affected_package_names().len(), 5);
    assert!(plan.reasons.contains(&ImpactReason::CargoLockChange));
    assert!(plan.proof_lanes.contains("invalidation"));
}

#[test]
fn build_script_change_invalidates_reverse_dependents() {
    let graph = common::graph();
    let plan = AffectedPlanner::new(&graph).plan(
        &ChangeSet::new(["crates/native/build.rs"]),
        &PlannerOptions::default(),
    );
    let names = plan.affected_package_names();
    assert!(names.contains("native"));
    assert!(names.contains("app"));
    assert!(plan.reasons.contains(&ImpactReason::BuildScriptChange));
}

#[test]
fn proc_macro_change_invalidates_consumers() {
    let graph = common::graph();
    let plan = AffectedPlanner::new(&graph).plan(
        &ChangeSet::new(["crates/macro_magic/src/lib.rs"]),
        &PlannerOptions::default(),
    );
    let names = plan.affected_package_names();
    assert!(names.contains("macro_magic"));
    assert!(names.contains("core"));
    assert!(names.contains("api"));
    assert!(names.contains("app"));
    assert!(plan.reasons.contains(&ImpactReason::ProcMacroChange));
}

#[test]
fn native_build_input_change_invalidates_native_dependents() {
    let graph = common::graph();
    let plan = AffectedPlanner::new(&graph).plan(
        &ChangeSet::new(["crates/native/native/build_input.conf"]),
        &PlannerOptions::default(),
    );
    let names = plan.affected_package_names();
    assert!(names.contains("native"));
    assert!(names.contains("app"));
    assert!(plan.reasons.contains(&ImpactReason::NativeDependencyChange));
}
