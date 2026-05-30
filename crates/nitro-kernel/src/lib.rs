#![forbid(unsafe_code)]
#![doc = "Phase 11 orchestration kernel that ties operations, compliance, lifecycle, tenant guard, and replay verification together."]

use compliance_export::{default_controls, export_bundle, validate_bundle, EvidenceRecord};
use lifecycle_manager::{default_phase11_migrations, plan_rollback, plan_upgrade};
use ops_automation::{evaluate_operations, ServiceSignal, SloThresholds};
use phase11_audit::{record, AuditKind, AuditLedger};
use phase11_core::{
    quote, ExportFormat, Finding, PolicyDecision, Report, Severity, TenantId, Version,
};
use replay_verifier::{fixture_claim, verify_claim};
use tenant_guard::{decide, QuotaLimit, QuotaUsage, Role, TenantAction, TenantPolicyInput};

/// Top-level readiness output.
#[derive(Debug, Clone)]
pub struct Phase11Readiness {
    pub tenant: String,
    pub overall: Report,
    pub audit_receipts_json: String,
    pub ops_json: String,
    pub compliance_json: String,
    pub lifecycle_json: String,
    pub rollback_json: String,
    pub replay_json: String,
}

impl Phase11Readiness {
    /// Stable JSON output.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"tenant\":{},\"overall\":{},\"ops\":{},\"compliance\":{},\"lifecycle\":{},\"rollback\":{},\"replay\":{},\"audit_receipts\":{}}}",
            quote(&self.tenant),
            self.overall.to_json(),
            self.ops_json,
            self.compliance_json,
            self.lifecycle_json,
            self.rollback_json,
            self.replay_json,
            self.audit_receipts_json
        )
    }
}

/// Build a complete Phase 11 readiness report with safe defaults.
pub fn readiness(tenant: TenantId, actor: &str) -> Phase11Readiness {
    let mut ledger = AuditLedger::new();

    let tenant_decision = decide(
        &TenantPolicyInput {
            tenant: tenant.clone(),
            actor: actor.to_string(),
            role: Role::Auditor,
            action: TenantAction::ExportCompliance,
            quota: QuotaLimit::default(),
            usage: QuotaUsage::zero(),
            break_glass_ticket: None,
        },
        &mut ledger,
    );

    let ops = evaluate_operations(
        &tenant,
        actor,
        &[
            ServiceSignal::new("jitforge-api", 85, 5, 12, 2),
            ServiceSignal::new("runnerd", 120, 0, 4, 3),
            ServiceSignal::new("cratevault", 95, 1, 7, 1),
        ],
        &SloThresholds::default(),
        &mut ledger,
    );

    let upgrade = plan_upgrade(
        &tenant,
        actor,
        Version::new(0, 10, 0),
        Version::new(0, 11, 0),
        default_phase11_migrations(),
        &mut ledger,
    );
    let rollback = plan_rollback(&tenant, actor, &upgrade, &mut ledger);

    let replay = verify_claim(&tenant, actor, &fixture_claim(), 2.0, &mut ledger);

    let evidence = vec![
        EvidenceRecord::from_text(
            "rbac_decision",
            policy_receipt(&tenant_decision).as_str(),
            "auditor export allowed",
        ),
        EvidenceRecord::from_text(
            "tenant_policy",
            policy_receipt(&tenant_decision).as_str(),
            "tenant policy enforced",
        ),
        EvidenceRecord::from_text(
            "upgrade_plan",
            upgrade.receipt_id.as_str(),
            &upgrade.to_json(),
        ),
        EvidenceRecord::from_text(
            "rollback_plan",
            rollback.receipt_id.as_str(),
            &rollback.to_json(),
        ),
        EvidenceRecord::from_text("ops_plan", ops.receipt_id.as_str(), &ops.to_json()),
        EvidenceRecord::from_text("ops_report", ops.receipt_id.as_str(), &ops.report.to_json()),
        EvidenceRecord::from_text(
            "replay_verdict",
            replay.receipt_id.as_str(),
            &replay.to_json(),
        ),
    ];
    let bundle = export_bundle(
        &tenant,
        actor,
        ExportFormat::Json,
        default_controls(),
        evidence,
        &mut ledger,
    );
    let compliance_report = validate_bundle(&bundle);
    let lifecycle_report = upgrade.validate();

    let mut overall = Report::new("phase11.readiness");
    merge_findings(
        &mut overall,
        "tenant",
        tenant_decision_to_finding(&tenant_decision),
    );
    for report in [
        &ops.report,
        &compliance_report,
        &lifecycle_report,
        &replay.report,
    ] {
        for finding in &report.findings {
            overall.push(finding.clone());
        }
    }
    if overall
        .findings
        .iter()
        .all(|f| f.severity == Severity::Info)
    {
        overall.push(Finding::new(
            "phase11.ready",
            Severity::Info,
            "Phase 11 readiness gates are satisfied",
        ));
    }

    let receipt = record(
        &mut ledger,
        &tenant,
        AuditKind::Readiness,
        actor,
        "phase11-readiness",
        vec![format!("state={}", overall.state.as_str())],
    );
    overall.push(
        Finding::new(
            "phase11.readiness_receipt",
            Severity::Info,
            "readiness receipt emitted",
        )
        .with_receipt(receipt),
    );

    Phase11Readiness {
        tenant: tenant.as_str().to_string(),
        overall,
        audit_receipts_json: ledger.to_json(),
        ops_json: ops.to_json(),
        compliance_json: bundle.to_json(),
        lifecycle_json: upgrade.to_json(),
        rollback_json: rollback.to_json(),
        replay_json: replay.to_json(),
    }
}

fn merge_findings(report: &mut Report, _prefix: &str, finding: Option<Finding>) {
    if let Some(finding) = finding {
        report.push(finding);
    }
}

fn tenant_decision_to_finding(decision: &PolicyDecision) -> Option<Finding> {
    match decision {
        PolicyDecision::Allow { receipt_id, .. } => Some(
            Finding::new(
                "tenant.allowed",
                Severity::Info,
                "tenant policy allowed compliance export",
            )
            .with_receipt(receipt_id.clone()),
        ),
        PolicyDecision::Deny { finding, .. } => Some(finding.clone()),
    }
}

fn policy_receipt(decision: &PolicyDecision) -> String {
    match decision {
        PolicyDecision::Allow { receipt_id, .. } => receipt_id.clone(),
        PolicyDecision::Deny { finding, .. } => finding.code.clone(),
    }
}

/// Produce a compact operator help string.
pub fn help() -> &'static str {
    "jit-phase11 commands: readiness | evidence | upgrade | replay"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_is_not_fail_closed_with_defaults() {
        let tenant = TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid"));
        let readiness = readiness(tenant, "ops-bot");
        assert!(!readiness.overall.fails_closed());
        assert!(readiness.to_json().contains("phase11.ready"));
        assert!(readiness.audit_receipts_json.contains("readiness"));
    }
}
