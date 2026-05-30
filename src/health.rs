//! Owner: Aggregate JeRyu health reporting
//! Proof: `cargo test -p jeryu --lib health`
//! Invariants: Health checks are read-only; CI mode never requires host-only secrets.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::dashboards::runners::{RunnersDashboard, RunnersSummary};
use crate::api::dashboards::source_doctor::{SourceDoctorDashboard, SourceDoctorSummary};
use crate::api::entity::{BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{ComponentHealth, RunnerHealth, TuiReadModel};
use crate::pool::{PoolReservedNode, PoolTopologyPlan};

const HEALTH_ROOT_DISK_WARNING_MIN_FREE_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const HEALTH_ROOT_DISK_CRITICAL_MIN_FREE_BYTES: u64 = 5 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthMode {
    Local,
    Ci,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckStatus {
    Ok,
    Degraded,
    Failed,
    Skipped,
}

impl HealthCheckStatus {
    fn is_failed(self) -> bool {
        matches!(self, Self::Failed)
    }

    fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub id: String,
    pub status: HealthCheckStatus,
    pub detail: String,
    pub duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthSummary {
    pub checks_total: usize,
    pub checks_ok: usize,
    pub checks_degraded: usize,
    pub checks_failed: usize,
    pub runner_active_total: Option<usize>,
    pub runner_desired_total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_utilization_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_idle_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_stuck_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub generated_at: DateTime<Utc>,
    pub mode: HealthMode,
    pub ok: bool,
    pub status: String,
    pub summary: HealthSummary,
    pub checks: Vec<HealthCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_topology: Option<PoolTopologyPlan>,
    #[serde(default)]
    pub reserved_runner_nodes: Vec<PoolReservedNode>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HealthOptions {
    pub ci: bool,
}

fn health_report_status(checks_failed: usize, checks_degraded: usize) -> String {
    if checks_failed > 0 {
        "blocked".to_string()
    } else if checks_degraded > 0 {
        "warning".to_string()
    } else {
        "healthy".to_string()
    }
}

pub async fn build_health_report(options: HealthOptions) -> HealthReport {
    let mut checks = Vec::new();
    let mut pool_topology = None;
    let mut reserved_runner_nodes = Vec::new();

    if options.ci {
        checks::collect_ci_checks(&mut checks).await;
    } else {
        checks::collect_local_checks(&mut checks, &mut pool_topology, &mut reserved_runner_nodes)
            .await;
    }

    let checks_total = checks.len();
    let checks_ok = checks.iter().filter(|check| check.status.is_ok()).count();
    let checks_failed = checks
        .iter()
        .filter(|check| check.status.is_failed())
        .count();
    let checks_degraded = checks
        .iter()
        .filter(|check| check.status == HealthCheckStatus::Degraded)
        .count();
    let ok = checks_failed == 0;
    let (runner_active_total, runner_desired_total) = pool_topology
        .as_ref()
        .map(|topology| (Some(topology.active_total), Some(topology.desired_total)))
        .unwrap_or((None, None));
    let (runner_utilization_ratio, runner_idle_count, runner_stuck_count) =
        runner_utilization_summary_from_checks(&checks);

    HealthReport {
        generated_at: Utc::now(),
        mode: if options.ci {
            HealthMode::Ci
        } else {
            HealthMode::Local
        },
        ok,
        status: health_report_status(checks_failed, checks_degraded),
        summary: HealthSummary {
            checks_total,
            checks_ok,
            checks_degraded,
            checks_failed,
            runner_active_total,
            runner_desired_total,
            runner_utilization_ratio,
            runner_idle_count,
            runner_stuck_count,
        },
        checks,
        pool_topology,
        reserved_runner_nodes,
    }
}

pub fn apply_report_to_read_model(report: &HealthReport, model: &mut TuiReadModel) {
    let degraded = report.summary.checks_degraded + report.summary.checks_failed;
    model.source_doctor = SourceDoctorDashboard {
        summary: Some(SourceDoctorSummary {
            sources_total: report.summary.checks_total as u32,
            sources_healthy: report.summary.checks_ok as u32,
            sources_degraded: degraded as u32,
            schema_drift_count: report
                .checks
                .iter()
                .filter(|check| check.id == "pipeline_doctor_schema")
                .filter(|check| check.status == HealthCheckStatus::Degraded)
                .count() as u32,
        }),
        ..SourceDoctorDashboard::default()
    };

    let total_runners = report
        .summary
        .runner_desired_total
        .map_or(0, |value| value)
        .try_into()
        .unwrap_or(u32::MAX);
    let active_runners = report
        .summary
        .runner_active_total
        .map_or(0, |value| value)
        .try_into()
        .unwrap_or(u32::MAX);
    model.runners = RunnersDashboard {
        summary: Some(RunnersSummary {
            total_runners,
            active_runners,
            paused_runners: 0,
            draining_runners: 0,
        }),
        ..RunnersDashboard::default()
    };
    model.mission.active_runners = active_runners;
    model.mission.total_runners = total_runners;
    model.system.runners = RunnerHealth {
        online: active_runners,
        idle: active_runners,
        busy: 0,
        degraded: degraded.try_into().unwrap_or(u32::MAX),
    };
    model.mission.overall = if report.ok {
        HealthLevel::Healthy
    } else {
        HealthLevel::Critical
    };
    model.mission.safe_to_code = report.ok;
    model.mission.top_blocker = report
        .checks
        .iter()
        .find(|check| check.status.is_failed())
        .map(|check| BlockerSummary {
            kind: "health".to_string(),
            severity: Severity::Critical,
            entity: Some(EntityRef::new(EntityKind::System, "health")),
            summary: check.detail.clone(),
            recommended_action: None,
        });

    model.system.gitlab = component_from_check("gitlab", report, "gitlab");
    model.system.database = component_from_check("database", report, "database");
    model.system.docker = component_from_check("docker", report, "docker");
    model.system.cache = component_from_check("cache", report, "host_doctor");
    model.system.vault = component_from_check("vault", report, "vault");
}

#[path = "health/checks.rs"]
mod checks;

#[allow(unused_imports)]
pub(crate) use checks::{
    check, component_from_check, root_disk_headroom_check_from_free_bytes,
    runner_drift_check_from_totals, runner_utilization_from_totals,
    runner_utilization_summary_from_checks,
};

#[cfg(test)]
mod tests;
