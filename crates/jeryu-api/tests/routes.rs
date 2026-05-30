use jeryu_api::Router;

#[test]
fn readiness_route_writes_audit_event() {
    let mut router = Router::default();
    let response = router.get("/api/phase10/ready");
    assert_eq!(response.status, 200);
    assert!(router.state.audit_log.verify());
    assert_eq!(router.state.audit_log.events().len(), 1);
}

#[test]
fn benchmark_scorecard_route_passes() {
    let mut router = Router::default();
    let response = router.get("/api/phase10/benchmarks/scorecard");
    assert_eq!(response.status, 200, "{}", response.body);
    assert!(response.body.contains("trusted-no-op-rust-pr"));
}

#[test]
fn replay_and_soak_routes_pass() {
    let mut router = Router::default();
    assert_eq!(router.get("/api/phase10/benchmarks/replay").status, 200);
    assert_eq!(router.get("/api/phase10/reliability/soak").status, 200);
}

#[test]
fn rbac_route_authorizes_default_admin() {
    let mut router = Router::default();
    let response = router.get("/api/phase10/rbac/self-test");
    assert_eq!(response.status, 200);
    assert!(response.body.contains("PublishBenchmark"));
}
