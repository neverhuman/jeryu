use super::*;
use anyhow::{Context, Result};
use tracing::info;

pub async fn trigger_production_promotion(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    version: Option<String>,
) -> Result<i64> {
    let report = build_release_status_report(
        db,
        ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.to_string()),
            sha: None,
            limit: 20,
        },
    )
    .await?;
    let view = report
        .recent
        .iter()
        .find(|view| {
            version
                .as_deref()
                .map(|wanted| view.attempt.version == wanted)
                .unwrap_or(true)
        })
        .context("no release attempt found for production promotion")?;
    if view.canary_state != "e2e-passed" {
        return Err(ReleaseError::CanaryGateRejected {
            version: view.attempt.version.clone(),
            state: view.canary_state.clone(),
        }
        .into());
    }

    // Phase 4: Admission Control Enforcement - C Artifact Handoff validation.
    let release_root = release_dir(&view.attempt.version);
    let c_handoff_path = release_root.join("rendered/c-handoff.json");
    let c_validation_path = release_root.join("c-validation.json");

    if !c_handoff_path.exists() {
        return Err(ReleaseError::MissingHandoff {
            version: view.attempt.version.clone(),
            path: c_handoff_path,
        }
        .into());
    }
    if !c_validation_path.exists() {
        return Err(ReleaseError::MissingValidation {
            version: view.attempt.version.clone(),
            path: c_validation_path,
        }
        .into());
    }

    let sha = view.attempt.sha.clone();
    if let Some(existing_id) =
        lookup::production_promotion_pipeline_id(client, project_id, ref_name, &sha).await?
    {
        info!(
            project_id,
            pipeline_id = existing_id,
            ref_name = %ref_name,
            sha = %sha,
            version = %view.attempt.version,
            "production-promotion pipeline already exists"
        );
        return Ok(existing_id);
    }

    crate::cache::ensure_root_disk_headroom(
        crate::cache::ROOT_DISK_HEADROOM_MIN_FREE_BYTES,
        "production promotion",
    )
    .await?;

    let release_version = view.attempt.version.clone();
    let release_pipeline_id_str = match view.attempt.release_pipeline_id {
        Some(id) => id.to_string(),
        None => String::new(),
    };
    let mut trigger_vars = vec![
        ("CI_PIPELINE_PRODUCT", "production-promotion"),
        ("JERYU_PROD_APPROVED", "1"),
        ("JERYU_RELEASE_SHA", sha.as_str()),
        ("JERYU_RELEASE_VERSION", release_version.as_str()),
    ];
    if !release_pipeline_id_str.is_empty() {
        trigger_vars.push((
            "JERYU_RELEASE_PIPELINE_ID",
            release_pipeline_id_str.as_str(),
        ));
        trigger_vars.push((
            "JERYU_RELEASE_PIPELINE_ID",
            release_pipeline_id_str.as_str(),
        ));
    }
    let pipeline_id = client
        .trigger_pipeline(project_id, ref_name, trigger_vars)
        .await?;

    db.attach_production_pipeline(project_id, ref_name, &sha, pipeline_id, "created")
        .await?;

    let _ = db
        .upsert_tracked_pipeline(&crate::state::TrackedPipeline {
            pipeline_id,
            project_id,
            ref_name: ref_name.to_string(),
            sha: sha.clone(),
            status: "created".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
        .await;

    Ok(pipeline_id)
}
