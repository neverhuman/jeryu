#![forbid(unsafe_code)]
#![doc = "Operational health evaluation, runbook matching, and remediation planning."]

use phase11_audit::{record, AuditKind, AuditLedger};
use phase11_core::{quote, Finding, HealthState, Report, Severity, TenantId};

/// A measured service signal.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceSignal {
    pub service: String,
    pub p95_ms: u64,
    pub error_rate_per_million: u32,
    pub queue_depth: u32,
    pub stale_heartbeat_seconds: u64,
}

impl ServiceSignal {
    /// Create a new signal.
    pub fn new(
        service: impl Into<String>,
        p95_ms: u64,
        error_rate_per_million: u32,
        queue_depth: u32,
        stale_heartbeat_seconds: u64,
    ) -> Self {
        Self {
            service: service.into(),
            p95_ms,
            error_rate_per_million,
            queue_depth,
            stale_heartbeat_seconds,
        }
    }
}

/// SLO threshold profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SloThresholds {
    pub max_p95_ms: u64,
    pub max_error_rate_per_million: u32,
    pub max_queue_depth: u32,
    pub max_stale_heartbeat_seconds: u64,
}

impl Default for SloThresholds {
    fn default() -> Self {
        Self {
            max_p95_ms: 1_000,
            max_error_rate_per_million: 1_000,
            max_queue_depth: 500,
            max_stale_heartbeat_seconds: 60,
        }
    }
}

/// Remediation action selected by a runbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemediationAction {
    pub name: String,
    pub safe_automatic: bool,
    pub command_hint: String,
}

impl RemediationAction {
    /// JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":{},\"safe_automatic\":{},\"command_hint\":{}}}",
            quote(&self.name),
            self.safe_automatic,
            quote(&self.command_hint)
        )
    }
}

/// Result of evaluating operations health.
#[derive(Debug, Clone, PartialEq)]
pub struct OpsPlan {
    pub report: Report,
    pub actions: Vec<RemediationAction>,
    pub receipt_id: String,
}

impl OpsPlan {
    /// JSON representation.
    pub fn to_json(&self) -> String {
        let actions = self
            .actions
            .iter()
            .map(RemediationAction::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"receipt_id\":{},\"report\":{},\"actions\":[{}]}}",
            quote(&self.receipt_id),
            self.report.to_json(),
            actions
        )
    }
}

/// Evaluate a set of signals and emit a receipt.
pub fn evaluate_operations(
    tenant: &TenantId,
    actor: &str,
    signals: &[ServiceSignal],
    thresholds: &SloThresholds,
    ledger: &mut AuditLedger,
) -> OpsPlan {
    let mut report = Report::new("phase11.operations");
    let mut actions = Vec::new();

    if signals.is_empty() {
        report.push(Finding::new(
            "ops.signals_missing",
            Severity::Blocked,
            "no service signals were supplied",
        ));
        actions.push(action(
            "hold-merges",
            false,
            "jit ops hold --reason missing-signals",
        ));
    }

    for signal in signals {
        if signal.p95_ms > thresholds.max_p95_ms {
            report.push(Finding::new(
                format!("ops.{}.latency", signal.service),
                Severity::Degraded,
                format!(
                    "p95 {} ms exceeds {} ms",
                    signal.p95_ms, thresholds.max_p95_ms
                ),
            ));
            actions.push(action(
                "scale-read-path",
                true,
                "jit ops scale api --ring current",
            ));
        }
        if signal.error_rate_per_million > thresholds.max_error_rate_per_million {
            report.push(Finding::new(
                format!("ops.{}.errors", signal.service),
                Severity::Critical,
                format!(
                    "error rate {} ppm exceeds {} ppm",
                    signal.error_rate_per_million, thresholds.max_error_rate_per_million
                ),
            ));
            actions.push(action(
                "page-owner",
                false,
                "jit ops page --service affected",
            ));
        }
        if signal.queue_depth > thresholds.max_queue_depth {
            report.push(Finding::new(
                format!("ops.{}.queue", signal.service),
                Severity::Degraded,
                format!(
                    "queue depth {} exceeds {}",
                    signal.queue_depth, thresholds.max_queue_depth
                ),
            ));
            actions.push(action(
                "shed-noncritical-work",
                true,
                "jit ops shed --class best-effort",
            ));
        }
        if signal.stale_heartbeat_seconds > thresholds.max_stale_heartbeat_seconds {
            report.push(Finding::new(
                format!("ops.{}.heartbeat", signal.service),
                Severity::Blocked,
                format!(
                    "heartbeat stale for {} seconds",
                    signal.stale_heartbeat_seconds
                ),
            ));
            actions.push(action(
                "revoke-runner-lease",
                true,
                "jit runner lease revoke --stale",
            ));
        }
    }

    if report.findings.is_empty() {
        report.push(Finding::new(
            "ops.healthy",
            Severity::Info,
            "all service signals are within thresholds",
        ));
    }

    let evidence = vec![
        format!("signals={}", signals.len()),
        format!("state={}", report.state.as_str()),
    ];
    let receipt_id = record(
        ledger,
        tenant,
        AuditKind::Operation,
        actor,
        "ops-evaluation",
        evidence,
    );
    OpsPlan {
        report,
        actions: dedupe_actions(actions),
        receipt_id,
    }
}

fn action(name: &str, safe_automatic: bool, command_hint: &str) -> RemediationAction {
    RemediationAction {
        name: name.to_string(),
        safe_automatic,
        command_hint: command_hint.to_string(),
    }
}

fn dedupe_actions(actions: Vec<RemediationAction>) -> Vec<RemediationAction> {
    let mut out = Vec::new();
    for action in actions {
        if !out
            .iter()
            .any(|existing: &RemediationAction| existing.name == action.name)
        {
            out.push(action);
        }
    }
    out
}

/// Convert a health state into whether mutating operations may continue.
pub fn mutation_gate(state: HealthState) -> bool {
    matches!(state, HealthState::Healthy | HealthState::Degraded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_heartbeat_blocks() {
        let tenant = TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid"));
        let mut ledger = AuditLedger::new();
        let plan = evaluate_operations(
            &tenant,
            "ops-bot",
            &[ServiceSignal::new("runnerd", 10, 0, 0, 120)],
            &SloThresholds::default(),
            &mut ledger,
        );
        assert!(plan.report.fails_closed());
        assert!(plan.actions.iter().any(|a| a.name == "revoke-runner-lease"));
        assert!(!mutation_gate(plan.report.state));
    }
}
