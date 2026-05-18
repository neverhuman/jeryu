use anyhow::Result;
use serde::Deserialize;
use tracing::{debug, error, info};

use super::super::EngineState;
use super::SharedState;
use crate::decision::{SupersedenceAction, SupersedenceDecision};
use crate::impact;
use crate::state::TrackedPipeline;

#[derive(Debug, Deserialize)]
pub(crate) struct PushHookPayload {
    project_id: i64,
    before: String,
    after: String,
    #[serde(rename = "ref")]
    ref_name: String,
}

pub(crate) async fn handle_push_event_from_body(
    state: SharedState,
    body: &str,
) -> Result<(), serde_json::Error> {
    let payload = serde_json::from_str::<PushHookPayload>(body)?;
    handle_push_event(state, payload).await;
    Ok(())
}

pub(crate) async fn handle_push_event(state: SharedState, payload: PushHookPayload) {
    let ref_name = super::normalize_ref(&payload.ref_name);
    info!(
        project_id = payload.project_id,
        ref_name = %ref_name,
        before = %payload.before,
        after = %payload.after,
        "intercepted Push Hook for Semantic CI Evaluation"
    );

    if ref_name.starts_with("jeryu-test-") {
        debug!(
            project_id = payload.project_id,
            ref_name = %ref_name,
            "skipping supersedence/impact for ephemeral test branch"
        );
        return;
    }

    if crate::decision::is_branch_creation_push(&payload.before) {
        debug!(
            project_id = payload.project_id,
            "semantic evaluation bypassed: branch creation event"
        );
        return;
    }

    if let Err(e) = handle_supersedence(&state, payload.project_id, &ref_name, &payload.after).await
    {
        error!(error = %e, project_id = payload.project_id, ref_name = %ref_name, "supersedence evaluation failed");
    }

    match impact::plan_for_push(
        &state.client,
        payload.project_id,
        &payload.before,
        &payload.after,
    )
    .await
    {
        Ok(plan) => {
            let payload_json = impact::render_plan_payload(&plan);
            if let Err(e) = state
                .db
                .append_event(
                    "impact_decision",
                    Some(payload.project_id),
                    None,
                    "engine",
                    &payload_json.to_string(),
                )
                .await
            {
                error!(error = %e, "failed to persist impact decision");
            }

            info!(
                project_id = payload.project_id,
                ref_name = %ref_name,
                lanes = ?plan.selected_lanes,
                recovery_path = plan.widened_to_full,
                "semantic CI impact plan computed"
            );

            if !plan.affected_paths.is_empty() {
                let vti_plan = crate::test_intel::planner::plan_tests(&plan.affected_paths);
                let vti_json = crate::test_intel::explain::explain_json(&vti_plan);
                let mode = format!("{:?}", vti_plan.mode);
                let subsystems = vti_plan.affected_subsystems.join(",");
                if let Err(e) = state
                    .db
                    .record_test_plan(
                        payload.project_id,
                        &payload.before,
                        &payload.after,
                        &mode,
                        vti_plan.confidence,
                        vti_plan.selected_tests.len() as i64,
                        vti_plan.skipped_subsystems.len() as i64,
                        &subsystems,
                        vti_plan.repair_reason(),
                        &vti_json.to_string(),
                    )
                    .await
                {
                    error!(error = %e, "failed to persist VTI test plan");
                } else {
                    info!(
                        project_id = payload.project_id,
                        mode = %mode,
                        confidence = vti_plan.confidence,
                        selected = vti_plan.selected_tests.len(),
                        skipped = vti_plan.skipped_subsystems.len(),
                        "VTI test plan recorded"
                    );
                }
            }
        }
        Err(e) => {
            error!(error = %e, project_id = payload.project_id, ref_name = %ref_name, "impact analysis failed");
        }
    }
}

async fn handle_supersedence(
    state: &EngineState,
    project_id: i64,
    ref_name: &str,
    newest_sha: &str,
) -> Result<()> {
    let pipelines = state
        .client
        .list_pipelines(project_id, Some(ref_name))
        .await?;

    for pipeline in pipelines {
        state
            .db
            .upsert_tracked_pipeline(&TrackedPipeline {
                pipeline_id: pipeline.id,
                project_id,
                ref_name: pipeline.ref_name.clone(),
                sha: pipeline.sha.clone(),
                status: pipeline.status.clone(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
            .await?;

        if pipeline.sha == newest_sha {
            continue;
        }

        if !matches!(pipeline.status.as_str(), "pending" | "running" | "created") {
            continue;
        }

        let decision = SupersedenceDecision {
            project_id,
            ref_name: ref_name.to_string(),
            newest_sha: newest_sha.to_string(),
            superseded_pipeline_id: pipeline.id,
            superseded_sha: pipeline.sha.clone(),
            action: SupersedenceAction::Cancel,
            reason: "newer commit superseded older in-flight pipeline on the same ref".to_string(),
        };

        state
            .db
            .append_event(
                "pipeline_superseded",
                Some(project_id),
                None,
                "engine",
                &serde_json::to_string(&decision)?,
            )
            .await?;

        state
            .client
            .cancel_pipeline(project_id, pipeline.id)
            .await?;
        state
            .db
            .append_event(
                "pipeline_cancel_requested",
                Some(project_id),
                None,
                "engine",
                &serde_json::json!({
                    "pipeline_id": pipeline.id,
                    "sha": pipeline.sha,
                    "ref_name": ref_name,
                })
                .to_string(),
            )
            .await?;
    }

    Ok(())
}
