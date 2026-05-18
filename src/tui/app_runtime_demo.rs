use super::*;

#[path = "app_runtime_demo_state.rs"]
mod app_runtime_demo_state;
use app_runtime_demo_state::build_demo_state;

pub(crate) fn apply_demo_fixture(app: &mut App) {
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    let attempt = crate::state::ReleaseAttempt {
        id: 42,
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        ref_name: "main".into(),
        sha: "9c3f2d4e0b9f5d1d7cc8".into(),
        version: "v3.0.1-demo".into(),
        upstream_pipeline_id: Some(8_012),
        upstream_status: "success".into(),
        release_pipeline_id: Some(8_013),
        release_pipeline_status: Some("running".into()),
        production_pipeline_id: Some(8_014),
        production_pipeline_status: Some("pending".into()),
        canary_status: "in-flight".into(),
        canary_started_at: Some(now_str.clone()),
        canary_finished_at: None,
        canary_note: Some("Evaluating telemetry and E2E readiness".into()),
        created_at: now_str.clone(),
        updated_at: now_str.clone(),
    };

    let release_status = release::ReleaseAttemptView {
        attempt: attempt.clone(),
        release_dir: "target/demo-release".into(),
        canary_state_path: "artifacts/canary.state".into(),
        gate_remote_canary_path: "artifacts/remote-canary.txt".into(),
        gate_canary_e2e_path: "artifacts/canary-e2e.txt".into(),
        gate_canary_telemetry_path: "artifacts/canary-telemetry.txt".into(),
        telemetry_diag_path: "artifacts/telemetry-diag.json".into(),
        canary_state: "in-flight".into(),
        eligibility: "high-confidence".into(),
        phase: Some("validation".into()),
        detail: Some("Demo release is active and collecting proof.".into()),
        state_status: Some("running".into()),
        has_remote_gate: true,
        has_telemetry_gate: true,
        has_e2e_gate: true,
        has_telemetry_diag: true,
        release_identity_ok: true,
        canary_public_url: Some("https://example.invalid/jeryu/demo-canary".into()),
    };

    let flow_jobs = vec![
        demo_job_event(
            9_001,
            "success",
            "policy-admission",
            "trusted",
            "sys-a1",
            Some(1.6),
            now_str.clone(),
        ),
        demo_job_event(
            9_002,
            "running",
            "build-image",
            "trusted",
            "sys-a2",
            Some(0.4),
            now_str.clone(),
        ),
        demo_job_event(
            9_003,
            "pending",
            "integration-tests",
            "trusted",
            "sys-a3",
            None,
            now_str.clone(),
        ),
        demo_job_event(
            9_004,
            "failed",
            "security-gate",
            "security",
            "sys-s1",
            Some(0.8),
            now_str.clone(),
        ),
        demo_job_event(
            9_005,
            "running",
            "e2e-canary",
            "trusted",
            "sys-a4",
            Some(0.9),
            now_str.clone(),
        ),
    ];
    let flow_graph = crate::tui::flow::builder::build_graph(8_013, flow_jobs.clone());
    let progress_pct: u16 = 68;

    let flow = crate::tui::flow::FlowSnapshot {
        generated_at: now,
        gitlab_online: true,
        active_pipelines: vec![crate::tui::flow::PipelineFlow {
            pipeline_id: 8_013,
            project_id: release::DEFAULT_RELEASE_PROJECT_ID,
            ref_name: "main".into(),
            sha: Some("9c3f2d4e0b9f5d1d7cc8".into()),
            status: "running".into(),
            graph: flow_graph,
            current_blocker: Some(9_004),
            critical_path: vec![9_002, 9_003, 9_004],
            eta: Some(crate::tui::flow::EtaEstimate {
                remaining_secs: 380,
                confidence: crate::tui::flow::EtaConfidence::Medium,
                reason: "security gate remediation path may be needed".into(),
            }),
            progress_pct,
        }],
        outdated: false,
        last_non_empty_at: Some(now),
        selected_pipeline_id: Some(8_013),
        release: Some(release_status.clone()),
        pools: vec![
            demo_pool(
                "trusted",
                9001,
                "token-trusted",
                "linux,x86_64,trusted",
                2,
                10,
                4,
                4,
                "trusted",
            ),
            demo_pool(
                "security",
                9002,
                "token-security",
                "linux,x86_64,security",
                1,
                4,
                2,
                2,
                "restricted",
            ),
            demo_pool(
                "research",
                9003,
                "token-research",
                "linux,arm64,research",
                1,
                3,
                1,
                1,
                "experimental",
            ),
        ],
        active_containers: 11,
        cache_metrics: crate::tui::flow::CacheMetrics {
            hot_usage_bytes: 24_311_008,
            hits: 1_102,
            objects: 2_900,
            singleflight_coalesced: 72,
            hit_ratio: 0.88,
            misses: 148,
            requests: 1_250,
        },
    };

    app.state = build_demo_state(
        now,
        now_str.clone(),
        flow_jobs,
        flow,
        release_status,
        progress_pct,
    );

    app.selected_job_index = 0;
    app.selected_pipeline_index = 0;
    app.selected_pool_index = 0;
    app.selected_test_index = 0;
    app.selected_test_history = None;
    app.selected_evidence_index = 0;
    app.test_view_mode = TestViewMode::Average;
    app.evidence_view_mode = EvidenceViewMode::Capsules;
    app.maximize_logs = false;
    app.log_scroll_offset = 0;
    app.follow_log_tail = true;
    app.command_palette_open = false;
    app.command_palette_query.clear();
    app.selected_palette_index = 0;
    app.tick_count = 0;
    app.log_target = Some(LogTarget {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        job_id: 9_002,
    });
    app.log_target_tx.send(app.log_target).ok();
    app.remember_selected_job();
}

pub(crate) fn tick_demo_state(app: &mut App) {
    app.tick_count += 1;
    let tc = app.tick_count;

    // Simulate logs tailing for job 9002 (the running one we start on)
    if tc.is_multiple_of(2)
        && let Some(target) = app.log_target
        && target.job_id == 9002
    {
        let num = tc / 2;
        let log_line = format!(
            "[demo] Processing signal block {} [batch-routing] ... ok\n",
            num
        );
        app.state.live_log.text.push_str(&log_line);
    }

    // Advance progress
    if tc.is_multiple_of(4) {
        if let Some(pipeline) = app.state.flow.active_pipelines.first_mut()
            && pipeline.progress_pct < 100
        {
            pipeline.progress_pct += 1;
        }
        if let Some(view) = app.state.pipeline_progress_view.as_mut()
            && view.overall_pct < 100
        {
            view.overall_pct += 1;
        }
    }

    // Change status from running to success around tick 25
    if tc == 25 {
        if let Some(job) = app.state.recent_jobs.iter_mut().find(|j| j.job_id == 9002) {
            job.status = "success".into();
        }
        for pipeline in &mut app.state.flow.active_pipelines {
            for node in pipeline.graph.nodes.iter_mut() {
                if node.job_id == Some(9002) {
                    node.status = "success".into();
                }
            }
        }
    }
}
