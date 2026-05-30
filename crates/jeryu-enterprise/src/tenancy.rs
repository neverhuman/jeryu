//! Tenant isolation.

/// Tenant identifier.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct TenantId(pub String);

impl TenantId {
    /// Construct a tenant id.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Tenant boundary assertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TenantBoundary {
    pub actor_tenant: TenantId,
    pub resource_tenant: TenantId,
}

impl TenantBoundary {
    /// True when the actor can touch the resource within tenant isolation rules.
    pub fn is_within_boundary(&self) -> bool {
        self.actor_tenant == self.resource_tenant
    }

    /// Explain the isolation decision.
    pub fn explain(&self) -> String {
        if self.is_within_boundary() {
            format!("tenant {} boundary satisfied", self.actor_tenant.0)
        } else {
            format!(
                "tenant boundary denied: actor={} resource={}",
                self.actor_tenant.0, self.resource_tenant.0
            )
        }
    }
}
