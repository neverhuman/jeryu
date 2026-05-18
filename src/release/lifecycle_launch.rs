use super::*;

pub async fn launch_canary_for_green_pipeline(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    sha: &str,
    pipeline_id: i64,
) -> Result<()> {
    let ref_name = ref_name.trim();
    if ref_name != "main" {
        return Ok(());
    }

    let version = render_release_version(sha);
    if pipeline_has_release_execution_jobs(client, project_id, pipeline_id).await? {
        info!(
            project_id,
            pipeline_id,
            ref_name = %ref_name,
            sha = %sha,
            "pipeline is already a release-execution pipeline; skipping canary trigger"
        );
        return Ok(());
    }

    let Some(latest) =
        latest_release_candidate_pipeline_for_ref(client, project_id, ref_name).await?
    else {
        return Ok(());
    };
    if latest.id != pipeline_id || latest.sha != sha {
        info!(
            project_id,
            pipeline_id,
            latest_pipeline_id = latest.id,
            latest_status = %latest.status,
            ref_name = %ref_name,
            sha = %sha,
            "upstream pipeline is no longer the latest successful ref state; skipping canary trigger"
        );
        return Ok(());
    }

    let explain = build_pipeline_explain_report(client, project_id, pipeline_id).await?;
    let extended_green =
        explain.extended.total == 0 || explain.extended.passed == explain.extended.total;
    if !explain.release_eligible || !extended_green {
        let note = format!(
            "full-build gate not satisfied: release_eligible={} extended={}/{} blocker={}",
            explain.release_eligible,
            explain.extended.passed,
            explain.extended.total,
            explain.current_blocker.as_deref().unwrap_or("none")
        );
        db.finish_release_canary(project_id, ref_name, sha, "blocked", Some(&note))
            .await?;
        warn!(
            project_id,
            pipeline_id,
            ref_name = %ref_name,
            sha = %sha,
            note = %note,
            "refusing automatic canary for incomplete full build"
        );
        return Ok(());
    }

    if !release_impacting_change(sha).await? {
        db.upsert_release_attempt(
            project_id,
            ref_name,
            sha,
            &version,
            Some(pipeline_id),
            "success",
            "skipped",
        )
        .await?;
        db.finish_release_canary(
            project_id,
            ref_name,
            sha,
            "skipped",
            Some("change-impact policy classified this commit as non-release-impacting"),
        )
        .await?;
        info!(
            project_id,
            pipeline_id,
            ref_name = %ref_name,
            sha = %sha,
            version = %version,
            "release-impact policy skipped automatic canary"
        );
        return Ok(());
    }

    let claimed = db
        .claim_release_canary(project_id, ref_name, sha, &version, Some(pipeline_id))
        .await?;
    if !claimed {
        info!(
            project_id,
            pipeline_id,
            ref_name = %ref_name,
            sha = %sha,
            version = %version,
            "release candidate already claimed or completed"
        );
        return Ok(());
    }

    info!(
        project_id,
        pipeline_id,
        ref_name = %ref_name,
        sha = %sha,
        version = %version,
        "upstream pipeline green; launching canary"
    );

    // Preflight: verify SSH/Vault/registry/disk before burning a pipeline slot.
    let pf = release_preflight(None).await;
    if !pf.ok {
        let blockers: Vec<String> = pf
            .blockers
            .iter()
            .map(|b| format!("[{}] {}", b.code, b.detail))
            .collect();
        let note = format!("release preflight failed: {}", blockers.join("; "));
        db.finish_release_canary(project_id, ref_name, sha, "blocked", Some(&note))
            .await?;
        warn!(project_id, pipeline_id, ref_name = %ref_name, sha = %sha, note = %note, "preflight blocked canary launch");
        return Ok(());
    }

    let image_handoff = upstream_image_handoff(client, project_id, pipeline_id).await?;
    let upstream_artifact_pipeline_id = image_handoff
        .as_ref()
        .map(|handoff| handoff.artifact_pipeline_id)
        .unwrap_or(pipeline_id);
    let upstream_pipeline_id = upstream_artifact_pipeline_id.to_string();
    let upstream_build_job_id = image_handoff
        .as_ref()
        .map(|handoff| handoff.build_job_id.to_string());
    let upstream_enclave_image_ref = image_handoff
        .as_ref()
        .map(|handoff| handoff.image_ref.clone());
    if let Some(handoff) = &image_handoff {
        info!(
            project_id,
            pipeline_id,
            artifact_pipeline_id = handoff.artifact_pipeline_id,
            build_job_id = handoff.build_job_id,
            image_ref = %handoff.image_ref,
            "upstream registry image handoff found; canary will skip enclave rebuild"
        );
    }
    let release_pipeline_id = match client
        .trigger_pipeline(project_id, ref_name, {
            let mut variables = vec![
                ("CI_PIPELINE_PRODUCT", "release-execution"),
                ("JERYU_CANARY_APPROVED", "1"),
                ("JERYU_UPSTREAM_PIPELINE_ID", upstream_pipeline_id.as_str()),
                ("JERYU_RELEASE_SHA", sha),
                ("JERYU_RELEASE_VERSION", version.as_str()),
            ];
            if let Some(job_id) = upstream_build_job_id.as_deref() {
                variables.push(("JERYU_UPSTREAM_BUILD_JOB_ID", job_id));
            }
            if let Some(image_ref) = upstream_enclave_image_ref.as_deref() {
                variables.push(("VEOX_PUBLISH_ENCLAVE_REF", image_ref));
            }
            variables
        })
        .await
    {
        Ok(pipeline_id) => pipeline_id,
        Err(err) => {
            let note = format!("release-execution trigger failed before attach: {err}");
            db.finish_release_canary(project_id, ref_name, sha, "failed", Some(&note))
                .await?;
            return Err(err)
                .with_context(|| format!("trigger release-execution pipeline for {sha}"));
        }
    };

    let _ = db
        .upsert_tracked_pipeline(&crate::state::TrackedPipeline {
            pipeline_id: release_pipeline_id,
            project_id,
            ref_name: ref_name.to_string(),
            sha: sha.to_string(),
            status: "created".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
        .await;

    db.attach_release_pipeline(project_id, ref_name, sha, release_pipeline_id, "pending")
        .await?;
    info!(
        project_id,
        upstream_pipeline_id = pipeline_id,
        upstream_artifact_pipeline_id,
        release_pipeline_id,
        ref_name = %ref_name,
        sha = %sha,
        version = %version,
        "triggered release-execution canary pipeline"
    );
    // Write release-lock.json before triggering so CI jobs can assert identity.
    let lock = ReleaseLock {
        schema: 1,
        release_version: version.clone(),
        product_sha: sha.to_string(),
        certifying_pipeline_id: pipeline_id,
        upstream_pipeline_id: upstream_artifact_pipeline_id,
        build_job_id: image_handoff.as_ref().map(|h| h.build_job_id),
        image_ref: upstream_enclave_image_ref.clone(),
        release_tool_sha: option_env!("VERGEN_GIT_SHA")
            .unwrap_or("unknown")
            .to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    write_release_lock(&version, &lock);

    Ok(())
}
