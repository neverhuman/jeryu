use jitforge_enterprise::RollbackPlan;

#[test]
fn upgrade_rollback_plan_is_reversible() {
    let plan = RollbackPlan::phase10();
    assert!(plan.is_reversible());
    assert!(plan
        .rollback_steps
        .iter()
        .any(|step| step == "restore-schema-snapshot"));
}
