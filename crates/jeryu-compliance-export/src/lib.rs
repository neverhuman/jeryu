#![forbid(unsafe_code)]
#![doc = "Compliance evidence bundle construction and validation."]

use jeryu_phase11_audit::{AuditKind, AuditLedger, record};
use jeryu_phase11_core::{
    Digest, ExportFormat, Finding, Report, Severity, TenantId, json_array, quote,
};

/// Compliance control families supported by Phase 11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFamily {
    AccessControl,
    ChangeManagement,
    IncidentResponse,
    Availability,
    Provenance,
    TenantIsolation,
}

impl ControlFamily {
    /// Stable representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccessControl => "access_control",
            Self::ChangeManagement => "change_management",
            Self::IncidentResponse => "incident_response",
            Self::Availability => "availability",
            Self::Provenance => "provenance",
            Self::TenantIsolation => "tenant_isolation",
        }
    }
}

/// One compliance control and its required evidence labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub id: String,
    pub family: ControlFamily,
    pub title: String,
    pub required_evidence: Vec<String>,
}

impl Control {
    /// Create a control.
    pub fn new(
        id: impl Into<String>,
        family: ControlFamily,
        title: impl Into<String>,
        required_evidence: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            family,
            title: title.into(),
            required_evidence,
        }
    }
}

/// A collected evidence record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub label: String,
    pub receipt_id: String,
    pub digest: Digest,
}

impl EvidenceRecord {
    /// Create evidence from text.
    pub fn from_text(label: impl Into<String>, receipt_id: impl Into<String>, text: &str) -> Self {
        Self {
            label: label.into(),
            receipt_id: receipt_id.into(),
            digest: Digest::from_text("evidence", text),
        }
    }

    /// JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"label\":{},\"receipt_id\":{},\"digest\":{}}}",
            quote(&self.label),
            quote(&self.receipt_id),
            quote(self.digest.as_str())
        )
    }
}

/// Exported bundle of controls and evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBundle {
    pub tenant: String,
    pub format: ExportFormat,
    pub controls: Vec<Control>,
    pub evidence: Vec<EvidenceRecord>,
    pub receipt_id: String,
    pub bundle_digest: Digest,
}

impl EvidenceBundle {
    /// JSON representation.
    pub fn to_json(&self) -> String {
        let controls = self
            .controls
            .iter()
            .map(|c| {
                format!(
                    "{{\"id\":{},\"family\":{},\"title\":{},\"required_evidence\":{}}}",
                    quote(&c.id),
                    quote(c.family.as_str()),
                    quote(&c.title),
                    json_array(&c.required_evidence)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let evidence = self
            .evidence
            .iter()
            .map(EvidenceRecord::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"tenant\":{},\"format\":{},\"receipt_id\":{},\"bundle_digest\":{},\"controls\":[{}],\"evidence\":[{}]}}",
            quote(&self.tenant),
            quote(self.format.as_str()),
            quote(&self.receipt_id),
            quote(self.bundle_digest.as_str()),
            controls,
            evidence
        )
    }
}

/// Standard Phase 11 controls.
pub fn default_controls() -> Vec<Control> {
    vec![
        Control::new(
            "P11-AC-001",
            ControlFamily::AccessControl,
            "Scoped admin and operator access",
            vec!["rbac_decision".into(), "tenant_policy".into()],
        ),
        Control::new(
            "P11-CM-001",
            ControlFamily::ChangeManagement,
            "Ringed reversible upgrades",
            vec!["upgrade_plan".into(), "rollback_plan".into()],
        ),
        Control::new(
            "P11-IR-001",
            ControlFamily::IncidentResponse,
            "Operational runbook receipts",
            vec!["ops_plan".into()],
        ),
        Control::new(
            "P11-AV-001",
            ControlFamily::Availability,
            "SLO evidence and fail-closed gates",
            vec!["ops_report".into()],
        ),
        Control::new(
            "P11-PV-001",
            ControlFamily::Provenance,
            "Replayable benchmark and release claims",
            vec!["replay_verdict".into()],
        ),
        Control::new(
            "P11-TI-001",
            ControlFamily::TenantIsolation,
            "Tenant quota and isolation enforcement",
            vec!["tenant_policy".into()],
        ),
    ]
}

/// Validate that each control's required evidence is present.
pub fn validate_bundle(bundle: &EvidenceBundle) -> Report {
    let mut report = Report::new("phase11.compliance");
    for control in &bundle.controls {
        for required in &control.required_evidence {
            if !bundle.evidence.iter().any(|e| &e.label == required) {
                report.push(Finding::new(
                    format!("compliance.{}.missing.{}", control.id, required),
                    Severity::Blocked,
                    format!(
                        "control {} is missing required evidence {}",
                        control.id, required
                    ),
                ));
            }
        }
    }
    if report.findings.is_empty() {
        report.push(Finding::new(
            "compliance.complete",
            Severity::Info,
            "all controls have required evidence",
        ));
    }
    report
}

/// Build and receipt a compliance export.
pub fn export_bundle(
    tenant: &TenantId,
    actor: &str,
    format: ExportFormat,
    controls: Vec<Control>,
    evidence: Vec<EvidenceRecord>,
    ledger: &mut AuditLedger,
) -> EvidenceBundle {
    let canonical = evidence
        .iter()
        .map(|e| format!("{}={}", e.label, e.digest.as_str()))
        .collect::<Vec<_>>()
        .join(";");
    let digest = Digest::from_text("bundle", &canonical);
    let labels = evidence.iter().map(|e| e.label.clone()).collect::<Vec<_>>();
    let receipt_id = record(
        ledger,
        tenant,
        AuditKind::ComplianceExport,
        actor,
        "phase11-jeryu_compliance_export",
        labels,
    );
    EvidenceBundle {
        tenant: tenant.as_str().to_string(),
        format,
        controls,
        evidence,
        receipt_id,
        bundle_digest: digest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_evidence_blocks_export_validation() {
        let tenant = TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid"));
        let mut ledger = AuditLedger::new();
        let bundle = export_bundle(
            &tenant,
            "audit-bot",
            ExportFormat::Json,
            default_controls(),
            Vec::new(),
            &mut ledger,
        );
        let report = validate_bundle(&bundle);
        assert!(report.fails_closed());
    }
}
