#![forbid(unsafe_code)]
#![doc = "Version lifecycle, upgrade ring, migration, and rollback planning."]

use phase11_audit::{record, AuditKind, AuditLedger};
use phase11_core::{json_array, quote, Finding, Report, Severity, TenantId, UpgradeRing, Version};

/// Migration step with idempotency and rollback metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStep {
    pub name: String,
    pub idempotency_key: String,
    pub forward: String,
    pub rollback: String,
    pub requires_quiescence: bool,
}

impl MigrationStep {
    /// Create a new step.
    pub fn new(
        name: impl Into<String>,
        idempotency_key: impl Into<String>,
        forward: impl Into<String>,
        rollback: impl Into<String>,
        requires_quiescence: bool,
    ) -> Self {
        Self {
            name: name.into(),
            idempotency_key: idempotency_key.into(),
            forward: forward.into(),
            rollback: rollback.into(),
            requires_quiescence,
        }
    }

    /// True if the step has rollback instructions.
    pub fn reversible(&self) -> bool {
        !self.rollback.trim().is_empty()
    }

    /// JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":{},\"idempotency_key\":{},\"forward\":{},\"rollback\":{},\"requires_quiescence\":{}}}",
            quote(&self.name), quote(&self.idempotency_key), quote(&self.forward), quote(&self.rollback), self.requires_quiescence
        )
    }
}

/// Ringed upgrade plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradePlan {
    pub from: Version,
    pub to: Version,
    pub rings: Vec<UpgradeRing>,
    pub migrations: Vec<MigrationStep>,
    pub receipt_id: String,
}

impl UpgradePlan {
    /// Validate the plan.
    pub fn validate(&self) -> Report {
        let mut report = Report::new("phase11.lifecycle");
        if self.to.to_string() == self.from.to_string() {
            report.push(Finding::new(
                "lifecycle.noop_upgrade",
                Severity::Warning,
                "upgrade target equals current version",
            ));
        }
        if self.rings != UpgradeRing::ordered().to_vec() {
            report.push(Finding::new(
                "lifecycle.rings_incomplete",
                Severity::Blocked,
                "upgrade must pass through all rings in order",
            ));
        }
        if self.migrations.is_empty() {
            report.push(Finding::new(
                "lifecycle.migrations_missing",
                Severity::Blocked,
                "upgrade plan must declare migrations",
            ));
        }
        for step in &self.migrations {
            if step.idempotency_key.trim().is_empty() {
                report.push(Finding::new(
                    format!("lifecycle.{}.idempotency_missing", step.name),
                    Severity::Blocked,
                    "migration lacks idempotency key",
                ));
            }
            if !step.reversible() {
                report.push(Finding::new(
                    format!("lifecycle.{}.rollback_missing", step.name),
                    Severity::Blocked,
                    "migration lacks rollback",
                ));
            }
        }
        if report.findings.is_empty() {
            report.push(Finding::new(
                "lifecycle.valid",
                Severity::Info,
                "upgrade plan is ringed and reversible",
            ));
        }
        report
    }

    /// JSON representation.
    pub fn to_json(&self) -> String {
        let rings = self
            .rings
            .iter()
            .map(|r| r.as_str().to_string())
            .collect::<Vec<_>>();
        let migrations = self
            .migrations
            .iter()
            .map(MigrationStep::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"from\":{},\"to\":{},\"rings\":{},\"receipt_id\":{},\"migrations\":[{}]}}",
            quote(&self.from.to_string()),
            quote(&self.to.to_string()),
            json_array(&rings),
            quote(&self.receipt_id),
            migrations
        )
    }
}

/// Rollback plan derived from an upgrade plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackPlan {
    pub target: Version,
    pub steps: Vec<String>,
    pub receipt_id: String,
}

impl RollbackPlan {
    /// JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"target\":{},\"receipt_id\":{},\"steps\":{}}}",
            quote(&self.target.to_string()),
            quote(&self.receipt_id),
            json_array(&self.steps)
        )
    }
}

/// Build and receipt an upgrade plan.
pub fn plan_upgrade(
    tenant: &TenantId,
    actor: &str,
    from: Version,
    to: Version,
    migrations: Vec<MigrationStep>,
    ledger: &mut AuditLedger,
) -> UpgradePlan {
    let rings = UpgradeRing::ordered().to_vec();
    let evidence = vec![
        format!("from={}", from),
        format!("to={}", to),
        format!("migrations={}", migrations.len()),
    ];
    let receipt_id = record(
        ledger,
        tenant,
        AuditKind::UpgradePlan,
        actor,
        "phase11-upgrade",
        evidence,
    );
    UpgradePlan {
        from,
        to,
        rings,
        migrations,
        receipt_id,
    }
}

/// Build and receipt a rollback plan from an upgrade plan.
pub fn plan_rollback(
    tenant: &TenantId,
    actor: &str,
    upgrade: &UpgradePlan,
    ledger: &mut AuditLedger,
) -> RollbackPlan {
    let mut steps = upgrade
        .migrations
        .iter()
        .rev()
        .map(|m| m.rollback.clone())
        .collect::<Vec<_>>();
    if steps.is_empty() {
        steps.push("no rollback steps available; hold rollout".to_string());
    }
    let receipt_id = record(
        ledger,
        tenant,
        AuditKind::RollbackPlan,
        actor,
        "phase11-rollback",
        vec![format!("target={}", upgrade.from)],
    );
    RollbackPlan {
        target: upgrade.from.clone(),
        steps,
        receipt_id,
    }
}

/// Default Phase 11 migrations.
pub fn default_phase11_migrations() -> Vec<MigrationStep> {
    vec![
        MigrationStep::new(
            "audit-ledger-index",
            "p11-audit-ledger-index-v1",
            "create audit receipt lookup index",
            "drop audit receipt lookup index",
            false,
        ),
        MigrationStep::new(
            "tenant-quota-ledger",
            "p11-tenant-quota-ledger-v1",
            "create tenant quota snapshots",
            "archive tenant quota snapshots",
            false,
        ),
        MigrationStep::new(
            "replay-receipt-catalog",
            "p11-replay-receipt-catalog-v1",
            "create replay receipt catalog",
            "drop replay receipt catalog",
            true,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_upgrade_is_valid_and_rollback_reverses() {
        let tenant = TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid"));
        let mut ledger = AuditLedger::new();
        let plan = plan_upgrade(
            &tenant,
            "release-bot",
            Version::new(0, 10, 0),
            Version::new(0, 11, 0),
            default_phase11_migrations(),
            &mut ledger,
        );
        assert!(!plan.validate().fails_closed());
        let rollback = plan_rollback(&tenant, "release-bot", &plan, &mut ledger);
        assert_eq!(rollback.steps.len(), 3);
        assert!(rollback.to_json().contains("0.10.0"));
    }
}
