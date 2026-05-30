//! SLO definitions and measurements.

/// One SLO objective.
#[derive(Clone, Debug, PartialEq)]
pub struct Slo {
    pub name: &'static str,
    pub objective: f64,
    pub window: &'static str,
    pub query: &'static str,
}

/// Runtime measurement for an SLO.
#[derive(Clone, Debug, PartialEq)]
pub struct SloMeasurement {
    pub name: String,
    pub value: f64,
    pub objective: f64,
}

impl SloMeasurement {
    /// True when the measurement meets the objective.
    pub fn passes(&self) -> bool {
        self.value >= self.objective
    }
}

/// Phase 10 SLOs.
pub const fn phase10_slos() -> [Slo; 6] {
    [
        Slo {
            name: "benchmark_replay_success",
            objective: 0.999,
            window: "30d",
            query: "sum(rate(bench_replay_ok[30d])) / sum(rate(bench_replay_total[30d]))",
        },
        Slo {
            name: "audit_log_append_success",
            objective: 0.9999,
            window: "30d",
            query: "sum(rate(audit_append_ok[30d])) / sum(rate(audit_append_total[30d]))",
        },
        Slo {
            name: "rbac_decision_latency",
            objective: 0.995,
            window: "7d",
            query: "histogram_quantile(0.995, rbac_decision_seconds_bucket) < 0.050",
        },
        Slo {
            name: "tenant_isolation_gate",
            objective: 1.0,
            window: "30d",
            query: "tenant_escape_total == 0",
        },
        Slo {
            name: "backup_restore_drill_success",
            objective: 0.99,
            window: "90d",
            query: "sum(restore_drill_ok) / sum(restore_drill_total)",
        },
        Slo {
            name: "upgrade_rollback_drill_success",
            objective: 0.99,
            window: "90d",
            query: "sum(rollback_drill_ok) / sum(rollback_drill_total)",
        },
    ]
}
