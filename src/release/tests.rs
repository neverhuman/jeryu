use super::*;
use std::collections::HashMap;

fn job(name: &str, status: &str, allow_failure: bool) -> Job {
    Job {
        id: 1,
        name: name.to_string(),
        status: status.to_string(),
        stage: "test".to_string(),
        allow_failure,
        pipeline_id: None,
        pipeline: None,
        ref_name: Some("main".to_string()),
        web_url: None,
        queued_duration: None,
        duration: None,
        started_at: None,
        finished_at: None,
        runner: None,
    }
}

/// Canonical "ready-to-promote" `ReleaseAttemptView` test fixture rooted at
/// `release_dir`. Tests mutate fields they care about rather than re-spelling
/// the struct.
fn sample_release_view(attempt: ReleaseAttempt, release_dir: &str) -> ReleaseAttemptView {
    ReleaseAttemptView {
        attempt,
        release_dir: release_dir.to_string(),
        canary_state_path: format!("{release_dir}/deploy-canary-c-state.json"),
        gate_remote_canary_path: format!("{release_dir}/gate-remote-canary.json"),
        gate_canary_e2e_path: format!("{release_dir}/gate-canary-e2e.json"),
        gate_canary_telemetry_path: format!("{release_dir}/gate-canary-telemetry.json"),
        telemetry_diag_path: format!("{release_dir}/gate-canary-telemetry-diagnostics.json"),
        canary_state: "e2e-passed".into(),
        eligibility: "ready".into(),
        phase: Some("e2e".into()),
        detail: None,
        state_status: Some("success".into()),
        has_remote_gate: true,
        has_telemetry_gate: true,
        has_e2e_gate: true,
        has_telemetry_diag: true,
        release_identity_ok: true,
        canary_public_url: Some("https://example.invalid".into()),
    }
}

/// Build a `CiSchemaJob` test fixture from common defaults. Tests pass the
/// fields that vary; everything else is filled in with neutral defaults.
#[allow(clippy::too_many_arguments)] // test fixture: flat positional schema by design
fn sample_ci_schema_job(
    id: &str,
    section: &str,
    summary: &str,
    runner: &str,
    kind: &str,
    component: &str,
    depends_on: Vec<String>,
    estimated_cost: &str,
) -> CiSchemaJob {
    CiSchemaJob {
        id: id.into(),
        lane: "release-blocking".into(),
        release_blocking: true,
        section: section.into(),
        summary: summary.into(),
        runner_tags: runner.into(),
        runner_pool: runner.into(),
        kind: kind.into(),
        component: component.into(),
        pipeline_product: "main-candidate".into(),
        evidence_driven: false,
        depends_on,
        evidence_outputs: vec![],
        estimated_cost: estimated_cost.into(),
    }
}

/// Recurring `compile-workspace` schema fixture used by lane/VTI tests.
fn compile_workspace_schema_job() -> CiSchemaJob {
    sample_ci_schema_job(
        "compile-workspace",
        "Clean Checkout",
        "workspace check",
        "build",
        "compile",
        "workspace-build",
        vec![],
        "medium",
    )
}

fn test_rust_nextest_schema_job() -> CiSchemaJob {
    sample_ci_schema_job(
        "test-rust-nextest-4",
        "Bootstrap Stability",
        "workspace nextest partition",
        "build",
        "contract",
        "workspace-nextest",
        vec!["compile-workspace".into()],
        "heavy",
    )
}

fn lint_shell_schema_job() -> CiSchemaJob {
    sample_ci_schema_job(
        "lint-shell",
        "Static Analysis",
        "shell lint",
        "default",
        "lint",
        "shell",
        vec![],
        "small",
    )
}

/// Canonical "passed canary" `ReleaseAttempt` test fixture. Tests that need a
/// different state mutate the returned value rather than re-spelling the struct.
fn sample_release_attempt(version: &str) -> ReleaseAttempt {
    ReleaseAttempt {
        id: 1,
        project_id: 2,
        ref_name: "main".into(),
        sha: "abcdef1234567890".into(),
        version: version.into(),
        upstream_pipeline_id: Some(77),
        upstream_status: "success".into(),
        release_pipeline_id: Some(88),
        release_pipeline_status: Some("success".into()),
        production_pipeline_id: None,
        production_pipeline_status: None,
        canary_status: "passed".into(),
        canary_started_at: Some("2026-04-16T00:00:00Z".into()),
        canary_finished_at: Some("2026-04-16T00:10:00Z".into()),
        canary_note: Some("done".into()),
        created_at: "2026-04-16T00:00:00Z".into(),
        updated_at: "2026-04-16T00:10:01Z".into(),
    }
}

#[test]
fn version_uses_sha_prefix() {
    assert_eq!(
        render_release_version("abcdef1234567890"),
        "ci-abcdef123456"
    );
}

#[test]
fn status_text_includes_state_paths() {
    let attempt = ReleaseAttempt {
        release_pipeline_status: Some("running".into()),
        canary_status: "running".into(),
        canary_finished_at: None,
        canary_note: Some("launching".into()),
        updated_at: "2026-04-16T00:00:01Z".into(),
        ..sample_release_attempt("ci-abcdef123456")
    };
    let report = ReleaseStatusReport {
        generated_at: "2026-04-16T00:00:02Z".into(),
        project_id: Some(2),
        ref_name: Some("main".into()),
        sha: None,
        limit: 5,
        total_attempts: 1,
        latest: Some(view_attempt(attempt.clone()).expect("view attempt")),
        recent: vec![view_attempt(attempt).expect("view attempt")],
    };
    let text = render_release_status_text(&report);
    assert!(text.contains("jeryu release status"));
    assert!(text.contains("Active:"));
    assert!(text.contains("deploy-canary-c-state.json"));
    assert!(text.contains("Phase:"));
    assert!(text.contains("Gates:"));
    assert!(text.contains("telemetry_diag="));
}

#[test]
fn auto_promotion_requires_complete_canary_evidence() {
    let attempt = sample_release_attempt("ci-abcdef123456");
    let view = sample_release_view(attempt, "/tmp/release");
    assert!(should_trigger_production_promotion_with_gate(&view, false));
}

#[test]
fn auto_promotion_stops_when_prod_gate_already_exists() {
    let version = "ci-abcdef123456";
    let attempt = sample_release_attempt(version);
    let view = sample_release_view(attempt, &format!("/tmp/{version}"));
    assert!(!should_trigger_production_promotion_with_gate(&view, true));
}

#[test]
fn reconcile_prefers_existing_release_attempt_until_prod_finishes() {
    let mut attempt = sample_release_attempt("ci-abcdef123456");
    assert!(should_resume_existing_release_attempt_for_reconcile(
        &attempt
    ));

    attempt.production_pipeline_status = Some("success".into());
    assert!(!should_resume_existing_release_attempt_for_reconcile(
        &attempt
    ));

    let mut skipped = sample_release_attempt("ci-abcdef123456");
    skipped.canary_status = "skipped".into();
    skipped.release_pipeline_id = Some(99);
    skipped.production_pipeline_status = None;
    assert!(!should_resume_existing_release_attempt_for_reconcile(
        &skipped
    ));
}

#[test]
fn should_resume_existing_release_attempt() {
    let attempt = sample_release_attempt("ci-abcdef123456");
    assert!(should_resume_existing_release_attempt_for_reconcile(
        &attempt
    ));

    let mut finished = attempt.clone();
    finished.production_pipeline_status = Some("success".into());
    assert!(!should_resume_existing_release_attempt_for_reconcile(
        &finished
    ));
}

#[test]
fn pipeline_explain_text_lists_all_item_sections() {
    let report = PipelineExplainReport {
        generated_at: "2026-04-16T00:00:00Z".into(),
        project_id: 2,
        pipeline_id: 88,
        pipeline_sha: "abcdef1234567890".into(),
        pipeline_ref: "main".into(),
        pipeline_status: "running".into(),
        release_critical: LaneProgress {
            passed: 1,
            total: 1,
            percent: 100.0,
        },
        extended: LaneProgress {
            passed: 0,
            total: 0,
            percent: 0.0,
        },
        research: LaneProgress {
            passed: 0,
            total: 0,
            percent: 0.0,
        },
        release_execution: LaneProgress {
            passed: 1,
            total: 2,
            percent: 50.0,
        },
        current_blocker: None,
        release_eligible: true,
        blocking_failed: vec![PipelineExplainItem {
            id: "build".into(),
            status: "failed".into(),
            stage: Some("test".into()),
            runner_pool: "build".into(),
            kind: "compile".into(),
            component: "workspace".into(),
            evidence_driven: false,
            estimated_cost: "medium".into(),
            evidence_outputs: vec![],
            depends_on: vec![],
        }],
        blocking_pending: vec![PipelineExplainItem {
            id: "lint".into(),
            status: "pending".into(),
            stage: None,
            runner_pool: "default".into(),
            kind: "lint".into(),
            component: "shell".into(),
            evidence_driven: true,
            estimated_cost: "small".into(),
            evidence_outputs: vec![],
            depends_on: vec!["build".into()],
        }],
        non_blocking_failed: vec![PipelineExplainItem {
            id: "docs".into(),
            status: "canceled".into(),
            stage: Some("verify".into()),
            runner_pool: "default".into(),
            kind: "docs".into(),
            component: "docs".into(),
            evidence_driven: false,
            estimated_cost: "small".into(),
            evidence_outputs: vec![],
            depends_on: vec![],
        }],
        non_blocking_pending: vec![],
        incomplete_milestones: vec![],
        untracked_jobs: vec![],
    };

    let text = render_pipeline_explain_text(&report);
    assert!(text.contains("Blocking failed:"));
    assert!(text.contains("Blocking pending:"));
    assert!(text.contains("Non-blocking failed:"));
    assert!(text.contains("build [build / compile / failed]"));
    assert!(text.contains("lint [default / lint / pending]"));
    assert!(text.contains("docs [default / docs / canceled]"));
}

#[test]
fn canary_gate_files_capture_complete_and_promotion_readiness() {
    let complete = CanaryGateFiles {
        remote: true,
        telemetry: true,
        e2e: true,
        validation: true,
        handoff: true,
        telemetry_diag: false,
    };
    assert!(complete.canary_complete());
    assert!(complete.promotion_ready());

    let incomplete = CanaryGateFiles {
        validation: false,
        ..complete
    };
    assert!(!incomplete.canary_complete());
    assert!(!incomplete.promotion_ready());
}

#[test]
fn omitted_jobs_do_not_count_as_pending_for_successful_pipeline() {
    let schema = vec![
        compile_workspace_schema_job(),
        test_rust_nextest_schema_job(),
    ];
    let mut statuses = HashMap::new();
    statuses.insert(
        "compile-workspace".to_string(),
        AggregatedPipelineJob {
            status: "success".into(),
            stage: Some("compile".into()),
        },
    );

    let lane = pipeline_lane_progress(&schema, &statuses, "release-blocking", "success");
    assert_eq!(lane.passed, 1);
    assert_eq!(lane.total, 1);
    assert_eq!(effective_job_status(None, "success"), "omitted");
    assert_eq!(effective_job_status(None, "running"), "pending");
}

#[test]
fn selected_vti_graph_omits_absent_schema_jobs() {
    let schema = vec![compile_workspace_schema_job(), lint_shell_schema_job()];
    let mut aggregated = HashMap::new();
    aggregated.insert(
        "compile-workspace".to_string(),
        AggregatedPipelineJob {
            status: "success".into(),
            stage: Some("compile".into()),
        },
    );
    let metadata = VtiGraphMetadata {
        selected_graph: true,
        materialized_jobs: HashSet::from(["compile-workspace".to_string()]),
    };

    apply_vti_selected_omissions(&schema, &metadata, &mut aggregated);

    assert_eq!(
        aggregated.get("lint-shell").map(|job| job.status.as_str()),
        Some("vti-skipped")
    );
    let lane = pipeline_lane_progress(&schema, &aggregated, "release-blocking", "running");
    assert_eq!(lane.passed, 1);
    assert_eq!(lane.total, 1);
}

#[test]
fn allow_failure_release_candidate_jobs_do_not_block_reconcile() {
    let jobs = vec![
        job("build-enclave-server", "success", true),
        job("test-local-built", "failed", true),
        job("test-local-rc", "success", true),
    ];

    assert!(jobs_materialize_release_candidate(&jobs));
    assert!(failed_release_candidate_jobs(&jobs).is_empty());
}

#[test]
fn hard_release_candidate_failures_still_block_reconcile() {
    let jobs = vec![
        job("build-enclave-server", "success", true),
        job("test-local-rc", "failed", false),
    ];

    assert_eq!(
        failed_release_candidate_jobs(&jobs),
        vec!["test-local-rc".to_string()]
    );
}
