use jeryu_obs::ReliabilitySoak;

#[test]
fn one_hundred_run_reliability_soak_passes() {
    let soak = ReliabilitySoak::phase10_100_run();
    assert_eq!(soak.runs.len(), 100);
    assert!(soak.passes_phase10_gate());
}
