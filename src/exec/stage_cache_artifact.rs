use anyhow::Result;

/// Stores the build artifact to the action cache and CAS after a cold execution.
pub(crate) async fn store_build_artifact(
    db: &crate::state::Db,
    job_id: i64,
    project_id: Option<i64>,
    unit: &crate::cache_brain::BuildUnit,
    sandbox_path: &str,
) -> Result<()> {
    let namespace = match unit.trust_tier {
        crate::policy::TrustTier::Trusted => "trusted",
        crate::policy::TrustTier::Untrusted => "untrusted",
        crate::policy::TrustTier::Quarantine => "quarantine",
    };
    let manifest = serde_json::json!({
        "unit_type": unit.unit_type,
        "environment_signature": unit.environment_signature,
        "scope": unit.scope,
        "job_id": job_id,
        "project_id": project_id,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let _ = db
        .upsert_action_cache(&unit.input_signature, &manifest.to_string(), namespace)
        .await;
    tracing::info!(
        "Populated action_cache for signature {} in namespace {}",
        unit.input_signature,
        namespace
    );

    let cas_dir = crate::config::data_dir()
        .join("cas")
        .join(&unit.input_signature);
    if let Ok(()) = tokio::fs::create_dir_all(&cas_dir).await {
        let payload_path = cas_dir.join("payload.tar.zst");
        let manifest_path = cas_dir.join("manifest.json");
        let archive_status = tokio::process::Command::new("tar")
            .arg("-I")
            .arg("zstd")
            .arg("-cf")
            .arg(&payload_path)
            .arg("-C")
            .arg(sandbox_path)
            .arg(".")
            .status()
            .await;
        match archive_status {
            Ok(s) if s.success() => {
                let _ = tokio::fs::write(&manifest_path, manifest.to_string()).await;
                tracing::info!("Archived build output to CAS: {:?}", cas_dir);
            }
            _ => {
                tracing::warn!(
                    "Failed to archive build output to CAS; future exact-hit will miss."
                );
            }
        }
    }

    Ok(())
}
