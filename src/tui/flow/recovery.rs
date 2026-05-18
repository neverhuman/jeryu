use super::builder::build_graph;
use super::model::PipelineFlow;
use crate::{
    gitlab_client::Job,
    state::JobEvent,
    tui::live::{is_live_job_status, is_terminal_job_status},
};

pub(crate) fn gitlab_job_to_event(project_id: i64, job: Job, now: &str) -> JobEvent {
    let pipeline_id = job.effective_pipeline_id();
    let pool_name = Some(
        match job.runner.as_ref().and_then(|r| r.description.as_deref()) {
            Some(desc) => desc.to_owned(),
            None => job.stage.clone(),
        },
    );
    let received_at = match (job.started_at.clone(), job.finished_at.clone()) {
        (Some(started_at), _) => started_at,
        (None, Some(finished_at)) => finished_at,
        (None, None) => now.to_string(),
    };

    JobEvent {
        job_id: job.id,
        project_id,
        pipeline_id,
        status: job.status,
        job_name: Some(job.name),
        pool_name,
        system_id: None,
        queued_duration: job.queued_duration,
        received_at,
    }
}

pub(crate) fn pipeline_flow_from_jobs(
    pipeline_id: i64,
    project_id: i64,
    ref_name: String,
    sha: Option<String>,
    status: String,
    jobs: Vec<JobEvent>,
) -> PipelineFlow {
    let graph = build_graph(pipeline_id, jobs);
    pipeline_flow_from_graph(pipeline_id, project_id, ref_name, sha, status, graph)
}

pub(crate) fn pipeline_flow_from_graph(
    pipeline_id: i64,
    project_id: i64,
    ref_name: String,
    sha: Option<String>,
    status: String,
    graph: super::model::FlowGraph,
) -> PipelineFlow {
    let total = graph.nodes.len();
    let completed = graph
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.status.as_str(),
                "success" | "failed" | "canceled" | "skipped"
            )
        })
        .count();
    let running = graph
        .nodes
        .iter()
        .filter(|node| is_live_job_status(node.status.as_str()))
        .count();

    let progress_pct = if total > 0 {
        let effective = completed as f64 + (running as f64 * 0.5);
        ((effective / total as f64) * 100.0) as u16
    } else {
        0
    };

    let current_blocker = graph
        .nodes
        .iter()
        .filter(|node| is_live_job_status(node.status.as_str()) || node.status == "failed")
        .max_by_key(|node| node.elapsed_secs)
        .and_then(|node| node.job_id);

    PipelineFlow {
        pipeline_id,
        project_id,
        ref_name,
        sha,
        status,
        graph,
        current_blocker,
        critical_path: vec![],
        eta: None,
        progress_pct,
    }
}

pub(crate) fn recover_flow_from_recent_jobs(
    project_id: i64,
    jobs: &[JobEvent],
) -> Option<PipelineFlow> {
    let mut pipeline_id = jobs.iter().find_map(|job| job.pipeline_id).unwrap_or(0);
    let mut live_jobs = jobs
        .iter()
        .filter(|job| job.pipeline_id == Some(pipeline_id) || pipeline_id == 0)
        .cloned()
        .collect::<Vec<_>>();

    if live_jobs.is_empty() {
        return None;
    }

    if pipeline_id == 0 {
        let has_live = live_jobs.iter().any(|job| {
            is_live_job_status(job.status.as_str()) || !is_terminal_job_status(job.status.as_str())
        });
        if !has_live {
            return None;
        }
    }

    if pipeline_id == 0 {
        pipeline_id = live_jobs
            .iter()
            .find_map(|job| job.pipeline_id)
            .unwrap_or(0);
    }

    if pipeline_id == 0 {
        return Some(pipeline_flow_from_jobs(
            0,
            project_id,
            "recent jobs".to_string(),
            None,
            "metadata-poor".to_string(),
            std::mem::take(&mut live_jobs),
        ));
    }

    Some(pipeline_flow_from_jobs(
        pipeline_id,
        project_id,
        "recent jobs".to_string(),
        None,
        "metadata-poor".to_string(),
        live_jobs,
    ))
}
