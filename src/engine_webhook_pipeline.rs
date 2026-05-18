use serde::Deserialize;
use tracing::{debug, error, info};

use super::SharedState;
use super::normalize_ref;
use crate::release;
use crate::state::TrackedPipeline;

#[derive(Debug, Deserialize)]
pub(crate) struct PipelineHookPayload {
    project: Option<ProjectInfo>,
    object_attributes: Option<PipelineAttributes>,
}

#[derive(Debug, Deserialize)]
struct ProjectInfo {
    id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct PipelineAttributes {
    id: Option<i64>,
    status: Option<String>,
    sha: Option<String>,
    #[serde(rename = "ref")]
    ref_name: Option<String>,
}

pub(crate) async fn handle_pipeline_event_from_body(
    state: SharedState,
    body: &str,
) -> Result<(), serde_json::Error> {
    let payload = serde_json::from_str::<PipelineHookPayload>(body)?;
    handle_pipeline_event(state, payload).await;
    Ok(())
}

pub(crate) async fn handle_pipeline_event(state: SharedState, payload: PipelineHookPayload) {
    if let Some(attrs) = payload.object_attributes {
        info!(
            pipeline_id = attrs.id,
            status = attrs.status,
            ref_name = attrs.ref_name,
            "pipeline event"
        );

        if let (Some(pipeline_id), Some(status), Some(ref_name), Some(sha)) =
            (attrs.id, attrs.status, attrs.ref_name, attrs.sha)
        {
            let ref_name = normalize_ref(&ref_name);
            let project_id = payload
                .project
                .and_then(|project| project.id)
                .unwrap_or_default();
            let _ = state
                .db
                .upsert_tracked_pipeline(&TrackedPipeline {
                    pipeline_id,
                    project_id,
                    ref_name: ref_name.clone(),
                    sha: sha.clone(),
                    status: status.clone(),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                })
                .await;

            if ref_name == "main" && status == "success" {
                if let Ok(Some(attempt)) = state
                    .db
                    .release_attempt_by_production_pipeline_id(pipeline_id)
                    .await
                {
                    if let Err(err) = state
                        .db
                        .update_production_pipeline_status(pipeline_id, &status)
                        .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "failed to refresh production pipeline status"
                        );
                    } else {
                        info!(
                            project_id,
                            pipeline_id,
                            sha = %attempt.sha,
                            version = %attempt.version,
                            "production-promotion pipeline passed"
                        );
                    }
                    return;
                }

                if let Ok(Some(attempt)) = state
                    .db
                    .release_attempt_by_release_pipeline_id(pipeline_id)
                    .await
                {
                    if let Err(err) = state
                        .db
                        .update_release_pipeline_status(pipeline_id, &status)
                        .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "failed to refresh release pipeline status"
                        );
                    } else {
                        info!(
                            project_id,
                            pipeline_id,
                            sha = %attempt.sha,
                            version = %attempt.version,
                            "release-execution pipeline passed"
                        );
                    }
                    let state = state.clone();
                    let ref_name = ref_name.clone();
                    tokio::spawn(async move {
                        if let Err(err) = release::maybe_trigger_production_promotion(
                            &state.db,
                            &state.client,
                            project_id,
                            &ref_name,
                            Some(&attempt.sha),
                            Some(&attempt.version),
                        )
                        .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                sha = %attempt.sha,
                                version = %attempt.version,
                                error = %err,
                                "automatic production promotion check failed"
                            );
                        }
                    });
                    return;
                }

                let state = state.clone();
                let ref_name = ref_name.clone();
                let sha = sha.clone();
                tokio::spawn(async move {
                    if let Err(err) = release::launch_canary_for_green_pipeline(
                        &state.db,
                        &state.client,
                        project_id,
                        &ref_name,
                        &sha,
                        pipeline_id,
                    )
                    .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "automatic canary launch failed"
                        );
                    }
                });
            } else if ref_name == "main"
                && matches!(status.as_str(), "failed" | "canceled" | "skipped")
            {
                match state
                    .db
                    .release_attempt_by_release_pipeline_id(pipeline_id)
                    .await
                {
                    Ok(Some(attempt)) => {
                        if let Err(err) = state
                            .db
                            .update_release_pipeline_status(pipeline_id, &status)
                            .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                status = %status,
                                error = %err,
                                "failed to refresh failed release-execution pipeline status"
                            );
                        } else {
                            let note = format!(
                                "release-execution pipeline {pipeline_id} ended with status {status}"
                            );
                            let _ = state
                                .db
                                .finish_release_canary(
                                    project_id,
                                    &ref_name,
                                    &attempt.sha,
                                    "failed",
                                    Some(&note),
                                )
                                .await;
                        }
                    }
                    Ok(None) => {
                        if let Ok(Some(_attempt)) = state
                            .db
                            .release_attempt_by_production_pipeline_id(pipeline_id)
                            .await
                            && let Err(err) = state
                                .db
                                .update_production_pipeline_status(pipeline_id, &status)
                                .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                status = %status,
                                error = %err,
                                "failed to refresh failed production-promotion pipeline status"
                            );
                        }
                    }
                    Err(err) => {
                        debug!(
                            project_id,
                            pipeline_id,
                            status = %status,
                            error = %err,
                            "pipeline was not a tracked release pipeline"
                        );
                    }
                }
            }
        }
    }
}
