//! Role-based access control.

use crate::tenancy::TenantId;

/// Permission granted by RBAC.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Permission {
    ReadBenchmark,
    PublishBenchmark,
    ReadAudit,
    AdminTenant,
    ManageSso,
    RunChaosDrill,
    RunUpgrade,
}

/// Named role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Role {
    pub name: String,
    pub permissions: Vec<Permission>,
}

impl Role {
    /// Tenant admin role.
    pub fn tenant_admin() -> Self {
        Self {
            name: "tenant-admin".to_owned(),
            permissions: vec![
                Permission::ReadBenchmark,
                Permission::PublishBenchmark,
                Permission::ReadAudit,
                Permission::AdminTenant,
                Permission::ManageSso,
                Permission::RunChaosDrill,
                Permission::RunUpgrade,
            ],
        }
    }

    /// Benchmark publisher role.
    pub fn benchmark_publisher() -> Self {
        Self {
            name: "benchmark-publisher".to_owned(),
            permissions: vec![Permission::ReadBenchmark, Permission::PublishBenchmark],
        }
    }

    /// Read-only auditor role.
    pub fn auditor() -> Self {
        Self {
            name: "auditor".to_owned(),
            permissions: vec![Permission::ReadBenchmark, Permission::ReadAudit],
        }
    }
}

/// Action being authorized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    ReadBenchmark,
    PublishBenchmark,
    ReadAudit,
    ManageTenant,
    ManageSso,
    RunChaosDrill,
    RunUpgrade,
}

impl Action {
    fn required_permission(self) -> Permission {
        match self {
            Self::ReadBenchmark => Permission::ReadBenchmark,
            Self::PublishBenchmark => Permission::PublishBenchmark,
            Self::ReadAudit => Permission::ReadAudit,
            Self::ManageTenant => Permission::AdminTenant,
            Self::ManageSso => Permission::ManageSso,
            Self::RunChaosDrill => Permission::RunChaosDrill,
            Self::RunUpgrade => Permission::RunUpgrade,
        }
    }
}

/// Protected resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resource {
    pub tenant: TenantId,
    pub kind: String,
    pub id: String,
}

/// RBAC policy binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RbacPolicy {
    pub actor: String,
    pub tenant: TenantId,
    pub roles: Vec<Role>,
}

/// Authorization decision with explainable reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationDecision {
    pub allowed: bool,
    pub reason: String,
}

impl RbacPolicy {
    /// Authorize an action against a tenant-scoped resource.
    pub fn authorize(&self, action: Action, resource: &Resource) -> AuthorizationDecision {
        if self.tenant != resource.tenant {
            return AuthorizationDecision {
                allowed: false,
                reason: "cross-tenant access denied".to_owned(),
            };
        }
        let required = action.required_permission();
        let allowed = self
            .roles
            .iter()
            .any(|role| role.permissions.contains(&required));
        if allowed {
            AuthorizationDecision {
                allowed: true,
                reason: format!("{} has {:?}", self.actor, required),
            }
        } else {
            AuthorizationDecision {
                allowed: false,
                reason: format!("{} lacks {:?}", self.actor, required),
            }
        }
    }
}
