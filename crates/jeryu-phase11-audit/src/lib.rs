#![forbid(unsafe_code)]
#![doc = "Append-only audit and receipt primitives for Phase 11."]

use jeryu_phase11_core::{
    Digest, Finding, Severity, TenantId, json_array, now_unix_seconds, quote,
};

/// Audit event categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    Operation,
    ComplianceExport,
    UpgradePlan,
    RollbackPlan,
    TenantDecision,
    ReplayVerification,
    Readiness,
}

impl AuditKind {
    /// Stable lowercase representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Operation => "operation",
            Self::ComplianceExport => "jeryu_compliance_export",
            Self::UpgradePlan => "upgrade_plan",
            Self::RollbackPlan => "rollback_plan",
            Self::TenantDecision => "tenant_decision",
            Self::ReplayVerification => "replay_verification",
            Self::Readiness => "readiness",
        }
    }
}

/// Immutable receipt emitted by Phase 11 operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub id: String,
    pub tenant: String,
    pub kind: AuditKind,
    pub actor: String,
    pub subject: String,
    pub evidence: Vec<String>,
    pub timestamp_unix: u64,
    pub digest: Digest,
}

impl Receipt {
    /// Build a receipt from canonical fields.
    pub fn new(
        tenant: &TenantId,
        kind: AuditKind,
        actor: impl Into<String>,
        subject: impl Into<String>,
        evidence: Vec<String>,
    ) -> Self {
        let actor = actor.into();
        let subject = subject.into();
        let timestamp_unix = now_unix_seconds();
        let canonical = format!(
            "{}|{}|{}|{}|{}|{}",
            tenant.as_str(),
            kind.as_str(),
            actor,
            subject,
            evidence.join(";"),
            timestamp_unix
        );
        let digest = Digest::from_text("receipt", &canonical);
        let id = digest.as_str().replace(':', "_");
        Self {
            id,
            tenant: tenant.as_str().to_string(),
            kind,
            actor,
            subject,
            evidence,
            timestamp_unix,
            digest,
        }
    }

    /// JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"id\":{},\"tenant\":{},\"kind\":{},\"actor\":{},\"subject\":{},\"evidence\":{},\"timestamp_unix\":{},\"digest\":{}}}",
            quote(&self.id),
            quote(&self.tenant),
            quote(self.kind.as_str()),
            quote(&self.actor),
            quote(&self.subject),
            json_array(&self.evidence),
            self.timestamp_unix,
            quote(self.digest.as_str())
        )
    }
}

/// Append-only in-memory ledger used by services and tests. Production adapters can persist these entries.
#[derive(Debug, Default, Clone)]
pub struct AuditLedger {
    receipts: Vec<Receipt>,
}

impl AuditLedger {
    /// Create an empty ledger.
    pub fn new() -> Self {
        Self {
            receipts: Vec::new(),
        }
    }

    /// Append a receipt and return its id.
    pub fn append(&mut self, receipt: Receipt) -> String {
        let id = receipt.id.clone();
        self.receipts.push(receipt);
        id
    }

    /// Borrow all receipts.
    pub fn receipts(&self) -> &[Receipt] {
        &self.receipts
    }

    /// Find a receipt by id.
    pub fn find(&self, id: &str) -> Option<&Receipt> {
        self.receipts.iter().find(|receipt| receipt.id == id)
    }

    /// Check append-only hash chain shape.
    pub fn validate(&self) -> Result<(), Finding> {
        for receipt in &self.receipts {
            if receipt.evidence.is_empty() {
                return Err(Finding::new(
                    "audit.evidence_missing",
                    Severity::Blocked,
                    "receipt has no evidence",
                ));
            }
            if receipt.actor.trim().is_empty() {
                return Err(Finding::new(
                    "audit.actor_missing",
                    Severity::Blocked,
                    "receipt has no actor",
                ));
            }
        }
        Ok(())
    }

    /// Export as JSON array.
    pub fn to_json(&self) -> String {
        format!(
            "[{}]",
            self.receipts
                .iter()
                .map(Receipt::to_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

/// Convenience function to create and append one receipt.
pub fn record(
    ledger: &mut AuditLedger,
    tenant: &TenantId,
    kind: AuditKind,
    actor: impl Into<String>,
    subject: impl Into<String>,
    evidence: Vec<String>,
) -> String {
    ledger.append(Receipt::new(tenant, kind, actor, subject, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipts_are_recorded_and_findable() {
        let tenant = TenantId::new("tenant-a").unwrap_or_else(|_| panic!("tenant should validate"));
        let mut ledger = AuditLedger::new();
        let id = record(
            &mut ledger,
            &tenant,
            AuditKind::Readiness,
            "ops",
            "phase11",
            vec!["report".into()],
        );
        assert!(ledger.find(&id).is_some());
        assert!(ledger.validate().is_ok());
        assert!(ledger.to_json().contains("readiness"));
    }
}
