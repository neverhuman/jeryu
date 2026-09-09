//! Findings, policy decisions, and deterministic reports.

use crate::json::quote;
use crate::severity::{HealthState, Severity};

/// A finding emitted by any Phase 11 subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub receipt_id: Option<String>,
}

impl Finding {
    /// Construct a finding.
    pub fn new(code: impl Into<String>, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            receipt_id: None,
        }
    }

    /// Attach a receipt id.
    pub fn with_receipt(mut self, receipt_id: impl Into<String>) -> Self {
        self.receipt_id = Some(receipt_id.into());
        self
    }

    /// Stable JSON representation.
    pub fn to_json(&self) -> String {
        let receipt = self
            .receipt_id
            .as_ref()
            .map(|id| quote(id))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"code\":{},\"severity\":{},\"message\":{},\"receipt_id\":{}}}",
            quote(&self.code),
            quote(self.severity.as_str()),
            quote(&self.message),
            receipt
        )
    }
}

/// Decision returned by policy gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow { reason: String, receipt_id: String },
    Deny { reason: String, finding: Finding },
}

impl PolicyDecision {
    /// True if the decision allows the action.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }
}

/// A compact report with deterministic ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub name: String,
    pub state: HealthState,
    pub findings: Vec<Finding>,
}

impl Report {
    /// New report.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: HealthState::Healthy,
            findings: Vec::new(),
        }
    }

    /// Add a finding and update state.
    pub fn push(&mut self, finding: Finding) {
        if finding.severity > self.highest_severity() {
            self.state = HealthState::from_severity(finding.severity);
        }
        self.findings.push(finding);
        self.findings.sort_by(|a, b| a.code.cmp(&b.code));
    }

    /// Highest severity in the report.
    pub fn highest_severity(&self) -> Severity {
        self.findings
            .iter()
            .map(|f| f.severity)
            .max()
            .unwrap_or(Severity::Info)
    }

    /// Fail closed when any finding is critical or blocked.
    pub fn fails_closed(&self) -> bool {
        self.highest_severity().fails_closed()
    }

    /// Stable JSON representation.
    pub fn to_json(&self) -> String {
        let findings = self
            .findings
            .iter()
            .map(Finding::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"name\":{},\"state\":{},\"highest_severity\":{},\"findings\":[{}]}}",
            quote(&self.name),
            quote(self.state.as_str()),
            quote(self.highest_severity().as_str()),
            findings
        )
    }
}
