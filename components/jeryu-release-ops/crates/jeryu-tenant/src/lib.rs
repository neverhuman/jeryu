#![forbid(unsafe_code)]
#![doc = "Tenant quotas, RBAC, isolation checks, and fail-closed policy decisions."]

use jeryu_phase11_audit::{AuditKind, AuditLedger, record};
use jeryu_phase11_core::{Finding, PolicyDecision, Severity, TenantId, quote};

/// Operator role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Viewer,
    Operator,
    Auditor,
    Admin,
    BreakGlass,
}

impl Role {
    /// Stable string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Auditor => "auditor",
            Self::Admin => "admin",
            Self::BreakGlass => "break_glass",
        }
    }
}

/// Tenant-scoped action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantAction {
    ReadEvidence,
    ExportCompliance,
    PlanUpgrade,
    ApplyUpgrade,
    PlanRollback,
    UpdateQuota,
    ReadReplay,
}

impl TenantAction {
    /// Stable string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadEvidence => "read_evidence",
            Self::ExportCompliance => "export_compliance",
            Self::PlanUpgrade => "plan_upgrade",
            Self::ApplyUpgrade => "apply_upgrade",
            Self::PlanRollback => "plan_rollback",
            Self::UpdateQuota => "update_quota",
            Self::ReadReplay => "read_replay",
        }
    }
}

/// Quota limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaLimit {
    pub max_repos: u32,
    pub max_runners: u32,
    pub max_storage_gib: u32,
    pub max_audit_exports_per_day: u32,
}

impl Default for QuotaLimit {
    fn default() -> Self {
        Self {
            max_repos: 10_000,
            max_runners: 1_000,
            max_storage_gib: 50_000,
            max_audit_exports_per_day: 100,
        }
    }
}

/// Current quota usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaUsage {
    pub repos: u32,
    pub runners: u32,
    pub storage_gib: u32,
    pub audit_exports_today: u32,
}

impl QuotaUsage {
    /// Empty usage.
    pub fn zero() -> Self {
        Self {
            repos: 0,
            runners: 0,
            storage_gib: 0,
            audit_exports_today: 0,
        }
    }
}

/// Tenant policy input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantPolicyInput {
    pub tenant: TenantId,
    pub actor: String,
    pub role: Role,
    pub action: TenantAction,
    pub quota: QuotaLimit,
    pub usage: QuotaUsage,
    pub break_glass_ticket: Option<String>,
}

impl TenantPolicyInput {
    /// JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"tenant\":{},\"actor\":{},\"role\":{},\"action\":{},\"repos\":{},\"runners\":{},\"storage_gib\":{},\"audit_exports_today\":{}}}",
            quote(self.tenant.as_str()),
            quote(&self.actor),
            quote(self.role.as_str()),
            quote(self.action.as_str()),
            self.usage.repos,
            self.usage.runners,
            self.usage.storage_gib,
            self.usage.audit_exports_today
        )
    }
}

/// Evaluate tenant access and quota policy. Missing or broad privileges deny by default.
pub fn decide(input: &TenantPolicyInput, ledger: &mut AuditLedger) -> PolicyDecision {
    if input.actor.trim().is_empty() {
        return deny("tenant.actor_missing", "actor is required");
    }
    if over_quota(&input.usage, &input.quota) {
        return deny("tenant.quota_exceeded", "tenant is over quota");
    }
    if input.role == Role::BreakGlass
        && input
            .break_glass_ticket
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        return deny(
            "tenant.break_glass_ticket_missing",
            "break-glass access requires a ticket",
        );
    }
    if !role_allows(input.role, input.action) {
        return deny("tenant.role_denied", "role does not allow requested action");
    }

    let receipt_id = record(
        ledger,
        &input.tenant,
        AuditKind::TenantDecision,
        &input.actor,
        input.action.as_str(),
        vec![
            format!("role={}", input.role.as_str()),
            "decision=allow".to_string(),
        ],
    );
    PolicyDecision::Allow {
        reason: "tenant policy allowed action".to_string(),
        receipt_id,
    }
}

fn deny(code: &str, message: &str) -> PolicyDecision {
    PolicyDecision::Deny {
        reason: message.to_string(),
        finding: Finding::new(code, Severity::Blocked, message),
    }
}

fn over_quota(usage: &QuotaUsage, quota: &QuotaLimit) -> bool {
    usage.repos > quota.max_repos
        || usage.runners > quota.max_runners
        || usage.storage_gib > quota.max_storage_gib
        || usage.audit_exports_today > quota.max_audit_exports_per_day
}

fn role_allows(role: Role, action: TenantAction) -> bool {
    match role {
        Role::Viewer => matches!(
            action,
            TenantAction::ReadEvidence | TenantAction::ReadReplay
        ),
        Role::Auditor => matches!(
            action,
            TenantAction::ReadEvidence | TenantAction::ExportCompliance | TenantAction::ReadReplay
        ),
        Role::Operator => matches!(
            action,
            TenantAction::ReadEvidence
                | TenantAction::PlanUpgrade
                | TenantAction::PlanRollback
                | TenantAction::ReadReplay
        ),
        Role::Admin => !matches!(action, TenantAction::ApplyUpgrade),
        Role::BreakGlass => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_cannot_update_quota() {
        let mut ledger = AuditLedger::new();
        let input = TenantPolicyInput {
            tenant: TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid")),
            actor: "alice".to_string(),
            role: Role::Viewer,
            action: TenantAction::UpdateQuota,
            quota: QuotaLimit::default(),
            usage: QuotaUsage::zero(),
            break_glass_ticket: None,
        };
        assert!(!decide(&input, &mut ledger).is_allowed());
    }

    #[test]
    fn auditor_can_export_with_receipt() {
        let mut ledger = AuditLedger::new();
        let input = TenantPolicyInput {
            tenant: TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid")),
            actor: "auditor".to_string(),
            role: Role::Auditor,
            action: TenantAction::ExportCompliance,
            quota: QuotaLimit::default(),
            usage: QuotaUsage::zero(),
            break_glass_ticket: None,
        };
        assert!(decide(&input, &mut ledger).is_allowed());
        assert_eq!(ledger.receipts().len(), 1);
    }

    #[test]
    fn empty_actor_denies_without_receipt() {
        let mut ledger = AuditLedger::new();
        let input = TenantPolicyInput {
            tenant: TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid")),
            actor: "  ".to_string(),
            role: Role::Admin,
            action: TenantAction::UpdateQuota,
            quota: QuotaLimit::default(),
            usage: QuotaUsage::zero(),
            break_glass_ticket: None,
        };
        let decision = decide(&input, &mut ledger);
        assert!(!decision.is_allowed());
        assert!(format!("{decision:?}").contains("tenant.actor_missing"));
        assert!(ledger.receipts().is_empty());
    }

    #[test]
    fn quota_excess_denies_before_role_check() {
        let mut ledger = AuditLedger::new();
        let input = TenantPolicyInput {
            tenant: TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid")),
            actor: "admin".to_string(),
            role: Role::Admin,
            action: TenantAction::UpdateQuota,
            quota: QuotaLimit {
                max_repos: 1,
                max_runners: 1,
                max_storage_gib: 1,
                max_audit_exports_per_day: 1,
            },
            usage: QuotaUsage {
                repos: 2,
                runners: 0,
                storage_gib: 0,
                audit_exports_today: 0,
            },
            break_glass_ticket: None,
        };
        let decision = decide(&input, &mut ledger);
        assert!(!decision.is_allowed());
        assert!(format!("{decision:?}").contains("tenant.quota_exceeded"));
    }

    #[test]
    fn break_glass_requires_ticket_then_allows_sensitive_action() {
        let mut ledger = AuditLedger::new();
        let mut input = TenantPolicyInput {
            tenant: TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid")),
            actor: "incident-commander".to_string(),
            role: Role::BreakGlass,
            action: TenantAction::ApplyUpgrade,
            quota: QuotaLimit::default(),
            usage: QuotaUsage::zero(),
            break_glass_ticket: None,
        };
        let denied = decide(&input, &mut ledger);
        assert!(!denied.is_allowed());
        assert!(ledger.receipts().is_empty());

        input.break_glass_ticket = Some("INC-1234".to_string());
        let allowed = decide(&input, &mut ledger);
        assert!(allowed.is_allowed());
        assert_eq!(ledger.receipts().len(), 1);
    }

    #[test]
    fn admin_cannot_apply_upgrade_without_break_glass() {
        let mut ledger = AuditLedger::new();
        let input = TenantPolicyInput {
            tenant: TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid")),
            actor: "admin".to_string(),
            role: Role::Admin,
            action: TenantAction::ApplyUpgrade,
            quota: QuotaLimit::default(),
            usage: QuotaUsage::zero(),
            break_glass_ticket: None,
        };
        let decision = decide(&input, &mut ledger);
        assert!(!decision.is_allowed());
        assert!(format!("{decision:?}").contains("tenant.role_denied"));
    }

    #[test]
    fn policy_input_json_names_role_action_and_usage() {
        let input = TenantPolicyInput {
            tenant: TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid")),
            actor: "auditor".to_string(),
            role: Role::Auditor,
            action: TenantAction::ReadReplay,
            quota: QuotaLimit::default(),
            usage: QuotaUsage {
                repos: 3,
                runners: 4,
                storage_gib: 5,
                audit_exports_today: 6,
            },
            break_glass_ticket: None,
        };
        let json = input.to_json();
        assert!(json.contains("\"role\":\"auditor\""));
        assert!(json.contains("\"action\":\"read_replay\""));
        assert!(json.contains("\"repos\":3"));
        assert!(json.contains("\"audit_exports_today\":6"));
    }
}
