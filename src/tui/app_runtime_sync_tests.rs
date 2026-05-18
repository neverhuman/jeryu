use super::{LiveLogState, PoolSyncMerge, TuiStateSnapshot};
use crate::state::JobEvent;
use crate::tui::flow::{FlowGraph, FlowSnapshot, PipelineFlow};
use crate::tui::live::live_job_status_rank;
use anyhow::Result;

fn job(job_id: i64, status: &str, received_at: &str) -> JobEvent {
    JobEvent {
        job_id,
        project_id: 2,
        pipeline_id: Some(10),
        status: status.into(),
        job_name: Some(format!("test-job-{job_id}")),
        pool_name: Some("default".into()),
        system_id: None,
        queued_duration: None,
        received_at: received_at.into(),
    }
}

fn job_without_pipeline(job_id: i64, status: &str, received_at: &str) -> JobEvent {
    JobEvent {
        pipeline_id: None,
        ..job(job_id, status, received_at)
    }
}

fn pool(name: &str, paused: bool) -> crate::state::Pool {
    crate::state::Pool {
        name: name.into(),
        gitlab_runner_id: 1,
        auth_token: "token".into(),
        tags: "linux".into(),
        executor: "docker".into(),
        min_warm: 1,
        max_managers: 4,
        concurrent: 2,
        request_concurrency: 1,
        paused,
        trust_tier: "trusted".into(),
    }
}

fn pipeline_flow(pipeline_id: i64) -> PipelineFlow {
    PipelineFlow {
        pipeline_id,
        project_id: 2,
        ref_name: "main".into(),
        sha: Some("abc123".into()),
        status: "running".into(),
        graph: FlowGraph::default(),
        current_blocker: None,
        critical_path: Vec::new(),
        eta: None,
        progress_pct: 50,
    }
}

#[test]
fn pool_sync_success_replaces_pools_and_clears_error() {
    let previous = vec![pool("cached", false)];
    let mut snapshot_pools = Vec::new();
    let mut sync_error = Some("old failure".to_string());

    let outcome = super::merge_pool_sync_result(
        &mut snapshot_pools,
        &mut sync_error,
        &previous,
        Ok(vec![pool("fresh", false), pool("paused", true)]),
    );

    assert_eq!(outcome, PoolSyncMerge::Fresh);
    assert_eq!(
        snapshot_pools
            .iter()
            .map(|pool| pool.name.as_str())
            .collect::<Vec<_>>(),
        vec!["fresh", "paused"]
    );
    assert!(sync_error.is_none());
}

#[test]
fn pool_sync_success_with_empty_list_is_a_real_empty_cluster() {
    let previous = vec![pool("cached", false)];
    let mut snapshot_pools = previous.clone();
    let mut sync_error = Some("old failure".to_string());

    let outcome =
        super::merge_pool_sync_result(&mut snapshot_pools, &mut sync_error, &previous, Ok(vec![]));

    assert_eq!(outcome, PoolSyncMerge::Fresh);
    assert!(snapshot_pools.is_empty());
    assert!(sync_error.is_none());
}

#[test]
fn pool_sync_failure_preserves_cached_pools_and_records_error() {
    let previous = vec![pool("cached-a", false), pool("cached-b", true)];
    let mut snapshot_pools = Vec::new();
    let mut sync_error = None;

    let outcome = super::merge_pool_sync_result(
        &mut snapshot_pools,
        &mut sync_error,
        &previous,
        Err(anyhow::anyhow!("redline pool scan timeout")),
    );

    assert_eq!(outcome, PoolSyncMerge::Stale);
    assert_eq!(
        snapshot_pools
            .iter()
            .map(|pool| pool.name.as_str())
            .collect::<Vec<_>>(),
        vec!["cached-a", "cached-b"]
    );
    let sync_error = sync_error.expect("pool sync failure should be recorded");
    assert!(sync_error.contains("pool sync failed"));
    assert!(sync_error.contains("redline pool scan timeout"));
}

#[test]
fn live_jobs_sort_running_ahead_of_created_and_pending() {
    let mut jobs = [
        JobEvent {
            job_id: 1,
            project_id: 2,
            pipeline_id: None,
            status: "created".into(),
            job_name: Some("build-enclave-server".into()),
            pool_name: Some("x86-64".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:00:00Z".into(),
        },
        JobEvent {
            job_id: 2,
            project_id: 2,
            pipeline_id: None,
            status: "waiting_for_resource".into(),
            job_name: Some("test-rust-nextest-1".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:01:00Z".into(),
        },
        JobEvent {
            job_id: 3,
            project_id: 2,
            pipeline_id: None,
            status: "running".into(),
            job_name: Some("test-rust-nextest-2".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:02:00Z".into(),
        },
        JobEvent {
            job_id: 4,
            project_id: 2,
            pipeline_id: None,
            status: "preparing".into(),
            job_name: Some("test-rust-nextest-3".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:03:00Z".into(),
        },
        JobEvent {
            job_id: 5,
            project_id: 2,
            pipeline_id: None,
            status: "running".into(),
            job_name: Some("test-rust-nextest-4".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:04:00Z".into(),
        },
        JobEvent {
            job_id: 6,
            project_id: 2,
            pipeline_id: None,
            status: "pending".into(),
            job_name: Some("test-rust-nextest-5".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:05:00Z".into(),
        },
    ];

    jobs.sort_by(|left, right| {
        live_job_status_rank(&right.status)
            .cmp(&live_job_status_rank(&left.status))
            .then_with(|| right.received_at.cmp(&left.received_at))
            .then_with(|| right.job_id.cmp(&left.job_id))
    });

    let statuses: Vec<_> = jobs.iter().map(|job| job.status.as_str()).collect();
    assert_eq!(
        statuses,
        vec![
            "running",
            "running",
            "preparing",
            "waiting_for_resource",
            "pending",
            "created"
        ]
    );
}

#[tokio::test]
async fn core_snapshot_preserves_flow_and_live_log_state() -> Result<()> {
    let mut app = super::test_app().await?;
    app.state.flow.outdated = true;
    app.state.live_log = LiveLogState {
        text: "running test output".into(),
        ..Default::default()
    };

    app.sync_tx.send(TuiStateSnapshot::default()).await.unwrap();
    app.tick().await;

    assert!(app.state.flow.outdated);
    assert_eq!(app.state.live_log.text, "running test output");
    Ok(())
}

#[tokio::test]
async fn empty_flow_snapshot_does_not_blank_existing_board() -> Result<()> {
    let mut app = super::test_app().await?;
    let generated_at = chrono::Utc::now();
    app.state.flow = FlowSnapshot {
        generated_at,
        active_pipelines: vec![pipeline_flow(42)],
        last_non_empty_at: Some(generated_at),
        ..Default::default()
    };

    app.flow_tx
        .send(FlowSnapshot {
            generated_at: generated_at + chrono::Duration::seconds(5),
            gitlab_online: true,
            ..Default::default()
        })
        .await
        .unwrap();
    app.tick().await;

    assert_eq!(app.state.flow.active_pipelines.len(), 1);
    assert_eq!(app.state.flow.active_pipelines[0].pipeline_id, 42);
    assert!(app.state.flow.outdated);
    assert_eq!(app.state.flow.last_non_empty_at, Some(generated_at));
    assert!(app.state.flow.gitlab_online);
    Ok(())
}

#[tokio::test]
async fn empty_flow_snapshot_uses_recent_jobs_before_collector_graph_arrives() -> Result<()> {
    let mut app = super::test_app().await?;
    let generated_at = chrono::Utc::now();
    app.state.recent_jobs = vec![
        JobEvent {
            job_id: 7,
            project_id: 2,
            pipeline_id: Some(55),
            status: "running".into(),
            job_name: Some("test-frontend-nht".into()),
            pool_name: Some("default".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:00:00Z".into(),
        },
        JobEvent {
            job_id: 8,
            project_id: 2,
            pipeline_id: Some(55),
            status: "created".into(),
            job_name: Some("test-local-rc".into()),
            pool_name: Some("build".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:01:00Z".into(),
        },
    ];

    app.flow_tx
        .send(FlowSnapshot {
            generated_at,
            gitlab_online: true,
            ..Default::default()
        })
        .await
        .unwrap();
    app.tick().await;

    assert_eq!(app.state.flow.active_pipelines.len(), 1);
    assert_eq!(app.state.flow.active_pipelines[0].pipeline_id, 55);
    assert_eq!(app.state.flow.active_pipelines[0].graph.nodes.len(), 2);
    assert!(app.state.flow.outdated);
    Ok(())
}

#[tokio::test]
async fn empty_flow_snapshot_recovers_live_jobs_without_pipeline_metadata() -> Result<()> {
    let mut app = super::test_app().await?;
    app.state.recent_jobs = vec![
        job_without_pipeline(1, "running", "2026-04-23T19:00:00Z"),
        job_without_pipeline(2, "preparing", "2026-04-23T19:01:00Z"),
    ];

    app.flow_tx
        .send(FlowSnapshot {
            generated_at: chrono::Utc::now(),
            gitlab_online: true,
            ..Default::default()
        })
        .await
        .unwrap();
    app.tick().await;

    assert_eq!(app.state.flow.active_pipelines.len(), 1);
    assert_eq!(app.state.flow.active_pipelines[0].pipeline_id, 0);
    assert_eq!(app.state.flow.active_pipelines[0].graph.nodes.len(), 2);
    assert!(app.state.flow.outdated);
    Ok(())
}

#[tokio::test]
async fn selected_job_survives_refresh_reorder() -> Result<()> {
    let mut app = super::test_app().await?;
    app.state.recent_jobs = vec![
        job(1, "running", "2026-04-23T19:00:00Z"),
        job(2, "pending", "2026-04-23T19:01:00Z"),
    ];
    app.selected_job_index = 1;
    app.remember_selected_job();

    let snap = TuiStateSnapshot {
        recent_jobs: vec![
            job(2, "running", "2026-04-23T19:02:00Z"),
            job(1, "success", "2026-04-23T19:03:00Z"),
        ],
        ..Default::default()
    };
    app.sync_tx.send(snap).await.unwrap();
    app.tick().await;

    assert_eq!(app.selected_job_index, 0);
    assert_eq!(app.selected_job().map(|job| job.job_id), Some(2));
    assert_eq!(app.log_target.map(|target| target.job_id), None);
    Ok(())
}

#[tokio::test]
async fn opening_and_scrolling_logs_controls_follow_mode() -> Result<()> {
    let mut app = super::test_app().await?;
    app.state.recent_jobs = vec![job(7, "running", "2026-04-23T19:00:00Z")];

    app.open_selected_job_log();
    assert!(app.maximize_logs);
    assert!(app.follow_log_tail);
    assert_eq!(app.log_target.map(|target| target.job_id), Some(7));

    app.scroll_logs_up(1);
    assert!(!app.follow_log_tail);

    app.follow_logs();
    assert!(app.follow_log_tail);
    assert_eq!(app.log_scroll_offset, u16::MAX);
    Ok(())
}
