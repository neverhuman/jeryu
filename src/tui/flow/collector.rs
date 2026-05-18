//! Owner: Interactive TUI subsystem — flow snapshot collector
//! Proof: `cargo nextest run -p jeryu -- tui::flow`
//! Invariants: Collection is best-effort, bounded, and never blocks the TUI render loop on remote state.

use super::model::{FlowSnapshot, PipelineFlow};
use super::recovery::{gitlab_job_to_event, pipeline_flow_from_jobs};
use crate::{
    docker::DockerCtl,
    gitlab_client::GitlabClient,
    release,
    state::{CiJobRun, JobEvent, TrackedPipeline, TuiSession}, // allowlist: TUI session import
};
use std::collections::BTreeSet;
use tokio::sync::mpsc;
use tokio::sync::watch;

pub async fn run_collector(
    session: TuiSession,
    docker: DockerCtl,
    gitlab: GitlabClient,
    tx: mpsc::Sender<FlowSnapshot>,
    _log_rx: watch::Receiver<Option<crate::tui::app::LogTarget>>,
) {
    let mut last_active_pipelines: Vec<PipelineFlow> = Vec::new();
    let mut last_active_seen_at: Option<chrono::DateTime<chrono::Utc>> = None;

    loop {
        let mut snap = collect_once(&session, &docker, &gitlab).await;

        if snap.active_pipelines.is_empty() {
            if !last_active_pipelines.is_empty() {
                snap.active_pipelines = last_active_pipelines.clone();
                snap.outdated = true;
                snap.last_non_empty_at = last_active_seen_at;
            }
        } else {
            last_active_pipelines = snap.active_pipelines.clone();
            last_active_seen_at = Some(snap.generated_at);
            snap.last_non_empty_at = last_active_seen_at;
        }

        if tx.send(snap).await.is_err() {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    }
}

pub async fn collect_once(
    session: &TuiSession,
    docker: &DockerCtl,
    gitlab: &GitlabClient,
) -> FlowSnapshot {
    let mut snap = FlowSnapshot {
        generated_at: chrono::Utc::now(),
        ..Default::default()
    };

    if let Ok(pools) = session.list_pools().await {
        snap.pools = pools;
    }

    if let Ok(managed) = docker.list_managed_containers().await {
        snap.active_containers = managed.len();
    }

    snap.gitlab_online = gitlab.is_ready().await;
    snap.release = release_hint(session).await;

    if let Ok(metrics) = session.get_cache_metrics().await {
        snap.cache_metrics.hot_usage_bytes = metrics.bytes_served;
        snap.cache_metrics.hits = metrics.hit_count;
        snap.cache_metrics.objects = metrics.object_count;
        snap.cache_metrics.singleflight_coalesced = metrics.singleflight_coalesced;
        snap.cache_metrics.hit_ratio = metrics.hit_ratio;
        snap.cache_metrics.misses = metrics.miss_count;
        snap.cache_metrics.requests = metrics.total_requests;
    }

    let mut included_pipeline_ids = BTreeSet::new();
    if let Ok(pipes) = session.list_tracked_pipelines(5).await {
        for pipeline in pipes {
            included_pipeline_ids.insert(pipeline.pipeline_id);
            snap.active_pipelines
                .push(build_tracked_pipeline_flow(session, gitlab, pipeline).await);
        }
    }

    if let Some((project_id, pipeline_id, ref_name, sha, status)) =
        release_pipeline_hint(session).await
        && included_pipeline_ids.insert(pipeline_id)
        && let Some(flow) =
            build_gitlab_pipeline_flow(gitlab, project_id, pipeline_id, ref_name, sha, status).await
    {
        snap.active_pipelines.insert(0, flow);
    }

    if snap.active_pipelines.is_empty() {
        snap.outdated = true;
    } else {
        snap.last_non_empty_at = Some(snap.generated_at);
    }

    snap
}

async fn build_tracked_pipeline_flow(
    session: &TuiSession,
    gitlab: &GitlabClient,
    pipeline: TrackedPipeline,
) -> PipelineFlow {
    let mut jobs = match session
        .list_ci_job_runs(pipeline.project_id, pipeline.pipeline_id)
        .await
    {
        Ok(runs) => runs.into_iter().map(ci_job_run_to_event).collect(),
        Err(_) => Vec::new(),
    };

    if jobs.is_empty()
        && let Ok(gitlab_jobs) = gitlab
            .list_pipeline_jobs_with_downstream(pipeline.project_id, pipeline.pipeline_id)
            .await
    {
        let now = chrono::Utc::now().to_rfc3339();
        jobs = gitlab_jobs
            .into_iter()
            .map(|job| gitlab_job_to_event(pipeline.project_id, job, &now))
            .collect();
    }

    pipeline_flow_from_jobs(
        pipeline.pipeline_id,
        pipeline.project_id,
        pipeline.ref_name,
        Some(pipeline.sha),
        pipeline.status,
        jobs,
    )
}

fn ci_job_run_to_event(run: CiJobRun) -> JobEvent {
    let received_at = match (run.started_at.clone(), run.finished_at.clone()) {
        (Some(started_at), _) => started_at,
        (None, Some(finished_at)) => finished_at,
        (None, None) => run.observed_at.clone(),
    };
    let pool_name = run.runner_pool.clone();
    JobEvent {
        job_id: run.job_id,
        project_id: run.project_id,
        pipeline_id: Some(run.pipeline_id),
        status: run.status,
        job_name: Some(run.job_name),
        pool_name,
        system_id: None,
        queued_duration: run.queued_duration_secs,
        received_at,
    }
}

async fn build_gitlab_pipeline_flow(
    gitlab: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    default_ref_name: String,
    default_sha: String,
    default_status: String,
) -> Option<PipelineFlow> {
    let pipeline = gitlab.get_pipeline(project_id, pipeline_id).await.ok();
    let jobs = gitlab
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await
        .ok()?;
    let now = chrono::Utc::now().to_rfc3339();
    let ref_name = match pipeline.as_ref() {
        Some(pipeline) => pipeline.ref_name.clone(),
        None => default_ref_name,
    };
    let sha = match pipeline.as_ref() {
        Some(pipeline) => pipeline.sha.clone(),
        None => default_sha,
    };
    let status = match pipeline.as_ref() {
        Some(pipeline) => pipeline.status.clone(),
        None => default_status,
    };

    Some(pipeline_flow_from_jobs(
        pipeline_id,
        project_id,
        ref_name,
        Some(sha),
        status,
        jobs.into_iter()
            .map(|job| gitlab_job_to_event(project_id, job, &now))
            .collect(),
    ))
}

async fn release_pipeline_hint(session: &TuiSession) -> Option<(i64, i64, String, String, String)> {
    let Ok(report) = release::build_release_status_report(
        session,
        release::ReleaseStatusQuery {
            project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
            ref_name: Some("main".into()),
            sha: None,
            limit: 1,
        },
    )
    .await
    else {
        return None;
    };

    report.latest.as_ref().and_then(|view| {
        view.attempt.release_pipeline_id.map(|pipeline_id| {
            (
                view.attempt.project_id,
                pipeline_id,
                view.attempt.ref_name.clone(),
                view.attempt.sha.clone(),
                match view.attempt.release_pipeline_status.clone() {
                    Some(s) => s,
                    None => "unknown".to_string(),
                },
            )
        })
    })
}

async fn release_hint(session: &TuiSession) -> Option<crate::release::ReleaseAttemptView> {
    let Ok(report) = release::build_release_status_report(
        session,
        release::ReleaseStatusQuery {
            project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
            ref_name: Some("main".into()),
            sha: None,
            limit: 1,
        },
    )
    .await
    else {
        return None;
    };
    report.latest
}
