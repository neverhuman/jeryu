use jitforge_api::Router;

#[test]
fn typed_api_reports_missing_github_compat_routes_as_not_implemented_yet() {
    let mut router = Router::default();
    let response = router.get("/repos/alice/missing/pulls");
    assert_eq!(response.status, 404);
    assert_eq!(response.body, "not found");
}

#[test]
fn typed_api_health_surface_is_current_phase10_surface() {
    let mut router = Router::default();
    let response = router.get("/api/phase10/ready");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, "phase10 ready");
    assert!(router.state.audit_log.verify());
}

#[test]
fn typed_api_exposes_benchmark_and_governance_routes() {
    let mut router = Router::default();
    assert_eq!(router.get("/api/phase10/benchmarks/scorecard").status, 200);
    assert_eq!(router.get("/api/phase10/benchmarks/replay").status, 200);
    assert_eq!(router.get("/api/phase10/rbac/self-test").status, 200);
}
