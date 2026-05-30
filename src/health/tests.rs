use super::*;

#[test]
fn report_is_healthy_when_only_degraded_checks_exist() {
    let report = HealthReport {
        generated_at: Utc::now(),
        mode: HealthMode::Ci,
        ok: true,
        status: health_report_status(0, 1),
        summary: HealthSummary {
            checks_total: 1,
            checks_ok: 0,
            checks_degraded: 1,
            checks_failed: 0,
            runner_active_total: None,
            runner_desired_total: None,
            runner_utilization_ratio: None,
            runner_idle_count: None,
            runner_stuck_count: None,
        },
        checks: vec![check(
            "pipeline_doctor_schema",
            HealthCheckStatus::Degraded,
            "schema unavailable".into(),
            0,
            None,
        )],
        pool_topology: None,
        reserved_runner_nodes: Vec::new(),
    };

    let mut model = TuiReadModel::default();
    apply_report_to_read_model(&report, &mut model);

    assert_eq!(report.status, "warning");
    assert_eq!(model.source_doctor.summary.unwrap().sources_degraded, 1);
    assert_eq!(model.mission.overall, HealthLevel::Healthy);
}

#[test]
fn health_report_status_reflects_warning_and_blocked_counts() {
    assert_eq!(health_report_status(0, 0), "healthy");
    assert_eq!(health_report_status(0, 1), "warning");
    assert_eq!(health_report_status(1, 0), "blocked");
    assert_eq!(health_report_status(2, 3), "blocked");
}

#[test]
fn failed_report_sets_mission_blocker() {
    let report = HealthReport {
        generated_at: Utc::now(),
        mode: HealthMode::Local,
        ok: false,
        status: "blocked".into(),
        summary: HealthSummary {
            checks_total: 1,
            checks_ok: 0,
            checks_degraded: 0,
            checks_failed: 1,
            runner_active_total: Some(39),
            runner_desired_total: Some(40),
            runner_utilization_ratio: Some(0.975),
            runner_idle_count: Some(1),
            runner_stuck_count: Some(0),
        },
        checks: vec![check(
            "pool_doctor",
            HealthCheckStatus::Failed,
            "topology drift".into(),
            0,
            None,
        )],
        pool_topology: None,
        reserved_runner_nodes: Vec::new(),
    };

    let mut model = TuiReadModel::default();
    apply_report_to_read_model(&report, &mut model);

    assert_eq!(model.mission.active_runners, 39);
    assert_eq!(model.mission.total_runners, 40);
    assert_eq!(model.mission.overall, HealthLevel::Critical);
    assert!(model.mission.top_blocker.is_some());
}

#[test]
fn runner_utilization_from_totals_reports_idle_and_stuck_counts() {
    let (ratio, idle_count, stuck_count) = runner_utilization_from_totals(8, 11);
    assert!((ratio - 1.375).abs() < f64::EPSILON);
    assert_eq!(idle_count, 3);
    assert_eq!(stuck_count, 0);

    let (ratio, idle_count, stuck_count) = runner_utilization_from_totals(8, 5);
    assert!((ratio - 0.625).abs() < f64::EPSILON);
    assert_eq!(idle_count, 0);
    assert_eq!(stuck_count, 3);
}

#[test]
fn runner_drift_check_from_totals_reports_drift() {
    let healthy = runner_drift_check_from_totals(8, 8, 2, 1);
    assert_eq!(healthy.status, HealthCheckStatus::Ok);
    assert!(healthy.detail.contains("in sync"));
    let healthy_data = healthy.data.clone().expect("expected drift data");
    assert_eq!(healthy_data["db_active_total"], 8);
    assert_eq!(healthy_data["live_running_total"], 8);
    assert_eq!(healthy_data["drift"], 0);
    assert_eq!(healthy_data["pool_count"], 2);
    let utilization = runner_utilization_summary_from_checks(&[healthy]);
    assert!((utilization.0.expect("utilization ratio") - 1.0).abs() < f64::EPSILON);
    assert_eq!(utilization.1, Some(0));
    assert_eq!(utilization.2, Some(0));

    let drifted = runner_drift_check_from_totals(8, 11, 2, 1);
    assert_eq!(drifted.status, HealthCheckStatus::Failed);
    assert!(drifted.detail.contains("delta=+3"));
    let drifted_data = drifted.data.clone().expect("expected drift data");
    assert_eq!(drifted_data["db_active_total"], 8);
    assert_eq!(drifted_data["live_running_total"], 11);
    assert_eq!(drifted_data["drift"], 3);
    assert_eq!(drifted_data["pool_count"], 2);
    let utilization = runner_utilization_summary_from_checks(&[drifted]);
    assert!((utilization.0.expect("utilization ratio") - 1.375).abs() < f64::EPSILON);
    assert_eq!(utilization.1, Some(3));
    assert_eq!(utilization.2, Some(0));

    let tolerated = runner_drift_check_from_totals(8, 9, 2, 1);
    assert_eq!(tolerated.status, HealthCheckStatus::Ok);
    assert!(tolerated.detail.contains("tolerated minor drift"));
    let tolerated_data = tolerated.data.clone().expect("expected drift data");
    assert_eq!(tolerated_data["db_active_total"], 8);
    assert_eq!(tolerated_data["live_running_total"], 9);
    assert_eq!(tolerated_data["drift"], 1);
    assert_eq!(tolerated_data["pool_count"], 2);
}

#[test]
fn root_disk_headroom_check_from_free_bytes_classifies_thresholds() {
    let healthy = root_disk_headroom_check_from_free_bytes(10_u64 * 1024 * 1024 * 1024, 1);
    assert_eq!(healthy.status, HealthCheckStatus::Ok);
    assert!(healthy.detail.contains("healthy"));
    let healthy_data = healthy.data.clone().expect("expected disk data");
    assert_eq!(healthy_data["available_bytes"], 10_u64 * 1024 * 1024 * 1024);
    assert_eq!(healthy_data["pressure"], "nominal");

    let warning = root_disk_headroom_check_from_free_bytes(9_u64 * 1024 * 1024 * 1024, 1);
    assert_eq!(warning.status, HealthCheckStatus::Degraded);
    assert!(warning.detail.contains("warning"));
    let warning_data = warning.data.clone().expect("expected disk data");
    assert_eq!(warning_data["pressure"], "warning");

    let critical = root_disk_headroom_check_from_free_bytes(4_u64 * 1024 * 1024 * 1024, 1);
    assert_eq!(critical.status, HealthCheckStatus::Failed);
    assert!(critical.detail.contains("critical"));
    let critical_data = critical.data.clone().expect("expected disk data");
    assert_eq!(critical_data["pressure"], "critical");
}
