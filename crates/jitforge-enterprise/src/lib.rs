//! Enterprise hardening primitives for JitForge Nitro Phase 10.

pub mod backup;
pub mod rbac;
pub mod security;
pub mod sso;
pub mod tenancy;
pub mod upgrade;

pub use backup::{BackupDrill, RestoreInvariant};
pub use rbac::{Action, AuthorizationDecision, Permission, RbacPolicy, Resource, Role};
pub use security::{RedTeamFinding, RedTeamSuite};
pub use sso::{OidcConfig, SamlConfig, SsoError};
pub use tenancy::{TenantBoundary, TenantId};
pub use upgrade::{RollbackPlan, UpgradePlan, Version};
