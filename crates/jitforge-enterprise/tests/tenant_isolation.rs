use jitforge_enterprise::{TenantBoundary, TenantId};

#[test]
fn tenant_boundary_blocks_cross_tenant_access() {
    let boundary = TenantBoundary {
        actor_tenant: TenantId::new("a"),
        resource_tenant: TenantId::new("b"),
    };
    assert!(!boundary.is_within_boundary());
    assert!(boundary.explain().contains("denied"));
}
