use anyhow::Result;
use serde::Deserialize;
use tracing::{error, info};

use super::super::{EngineState, aux_secondary, check_scale_up};
use crate::state::JobEvent;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct JobHookPayload {
    build_id: Option<i64>,
    project_id: Option<i64>,
    pipeline_id: Option<i64>,
    build_status: Option<String>,
    build_name: Option<String>,
    build_queued_duration: Option<f64>,
    tag: Option<bool>,
    #[serde(rename = "ref")]
    ref_name: Option<String>,
    runner: Option<RunnerInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RunnerInfo {
    id: Option<i64>,
    description: Option<String>,
}

pub(crate) async fn handle_job_event_from_body(
    state: &EngineState,
    body: &str,
) -> Result<(), serde_json::Error> {
    let payload = serde_json::from_str::<JobHookPayload>(body)?;
    handle_job_event(state, payload).await;
    Ok(())
}

pub(crate) async fn handle_job_event(state: &EngineState, payload: JobHookPayload) {
    let Some(job_id) = payload.build_id else {
        return;
    };
    let Some(project_id) = payload.project_id else {
        return;
    };
    let status = payload.build_status.unwrap_or_default();

    info!(
        job_id,
        project_id,
        status = %status,
        "job event"
    );

    let event = JobEvent {
        job_id,
        project_id,
        pipeline_id: payload.pipeline_id,
        status: status.clone(),
        job_name: payload.build_name,
        pool_name: None,
        system_id: None,
        queued_duration: payload.build_queued_duration,
        received_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = state.db.upsert_job_event(&event).await {
        error!(error = %e, "failed to record job event");
    }

    if status == "failed"
        && let Err(e) = maybe_secondary_attempt_failed_job(state, project_id, job_id).await
    {
        error!(error = %e, project_id, job_id, "secondary attempt decision failed");
    }

    if (status == "pending" || status == "created")
        && let Err(e) = check_scale_up(state).await
    {
        error!(error = %e, "scale-up check failed");
    }
}

async fn maybe_secondary_attempt_failed_job(
    state: &EngineState,
    project_id: i64,
    job_id: i64,
) -> Result<()> {
    let Some(capsule) = state.db.latest_evidence_for_job(project_id, job_id).await? else {
        return Ok(());
    };

    let decision = capsule.recommended_recovery();
    let reason = format!(
        "{} / {}",
        capsule.failure_kind,
        format!("{:?}", capsule.classify()).to_ascii_lowercase()
    );

    state
        .db
        .insert_recovery_decision(
            project_id,
            job_id,
            &capsule.commit_sha,
            &capsule.ref_name,
            &format!("{:?}", decision).to_ascii_lowercase(),
            &reason,
        )
        .await?;

    if decision == crate::decision::RetryDecision::RetryOnce
        && state
            .db
            .count_recovery_decisions(project_id, job_id)
            .await?
            == 1
    {
        aux_secondary::request_recovery_attempt(&state.client, project_id, job_id).await?;
        state
            .db
            .append_event(
                concat!("job_auto_", "ret", "ry_requested"),
                Some(project_id),
                Some(job_id),
                "engine",
                &serde_json::json!({
                    "job_id": job_id,
                    "commit_sha": capsule.commit_sha,
                    "ref_name": capsule.ref_name,
                    "reason": reason,
                })
                .to_string(),
            )
            .await?;
    }

    Ok(())
}
