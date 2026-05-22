use super::*;
use crate::decision::{SupersedenceAction, SupersedenceDecision};
use crate::state::TrackedPipeline;

pub(crate) async fn handle_supersedence(
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
