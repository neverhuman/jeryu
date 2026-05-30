use jeryu_obs::phase10_grafana_dashboard;

#[test]
fn dashboard_mentions_phase10_slos() {
    let json = phase10_grafana_dashboard();
    assert!(json.contains("Jeryu Phase 10"));
    assert!(json.contains("benchmark_replay_success"));
    assert!(json.contains("tenant_isolation_gate"));
}
