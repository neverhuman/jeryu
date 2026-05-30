use jeryu_enterprise::{Action, RbacPolicy, Resource, Role, TenantId};

#[test]
fn rbac_allows_in_tenant_permission_and_denies_cross_tenant() {
    let policy = RbacPolicy {
        actor: "alice".to_owned(),
        tenant: TenantId::new("tenant-a"),
        roles: vec![Role::benchmark_publisher()],
    };
    let same = Resource {
        tenant: TenantId::new("tenant-a"),
        kind: "benchmark".to_owned(),
        id: "bench1".to_owned(),
    };
    let other = Resource {
        tenant: TenantId::new("tenant-b"),
        kind: "benchmark".to_owned(),
        id: "bench1".to_owned(),
    };
    assert!(policy.authorize(Action::PublishBenchmark, &same).allowed);
    assert!(!policy.authorize(Action::PublishBenchmark, &other).allowed);
}

#[test]
fn auditor_cannot_manage_sso() {
    let policy = RbacPolicy {
        actor: "auditor".to_owned(),
        tenant: TenantId::new("tenant-a"),
        roles: vec![Role::auditor()],
    };
    let resource = Resource {
        tenant: TenantId::new("tenant-a"),
        kind: "sso".to_owned(),
        id: "primary".to_owned(),
    };
    assert!(!policy.authorize(Action::ManageSso, &resource).allowed);
}
