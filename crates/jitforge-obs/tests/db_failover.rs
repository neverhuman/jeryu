use jitforge_obs::ChaosDrill;

#[test]
fn db_failover_drill_passes_without_data_loss() {
    let result = ChaosDrill::db_failover().simulate(24_000, false);
    assert!(result.passed(), "{:?}", result);
}
