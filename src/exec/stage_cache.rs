use anyhow::Result;

use super::support::env_string_or_default;

/// Determines the build unit for the given stage, runs the cache verdict, and
/// exits the process early on an exact cache hit. Returns the build unit so the
/// caller can populate the artifact cache after a cold execution.
pub(super) async fn resolve_build_unit(
    db: &crate::state::Db,
    job_id: i64,
    project_id: Option<i64>,
    stage: &str,
    script_path: &str,
    sandbox_path: &str,
) -> Result<Option<crate::cache_brain::BuildUnit>> {
    if stage != "build_script"
        || std::env::var("CUSTOM_ENV_JERYU_FORCE_REFRESH")
            .ok()
            .as_deref()
            == Some("1")
    {
        return Ok(None);
    }

    let epoch_manager = crate::epoch::EpochManager::with_backend(db.pool(), db.backend());
    let taint_manager = crate::taint::TaintManager::with_backend(db.pool(), db.backend());
    let store = cache_brain_adapter::create_action_store(
        db.pool(),
        cache_brain_adapter::AdapterBackend::RedlineDb,
    );
    let cache_brain =
        crate::cache_brain::CacheBrain::with_store(epoch_manager, taint_manager, store);

    let is_rust_build = script_path.contains("cargo build")
        && !script_path.contains("cargo test")
        && !script_path.contains("cargo check")
        && !script_path.contains("cargo clippy");
    let dockerfile = std::path::Path::new(sandbox_path).join("Dockerfile");

    let build_unit: Option<crate::cache_brain::BuildUnit> = if dockerfile.exists() {
        if let Ok(witness) = crate::witness::WitnessBuilder::docker_build_witness(
            project_id.unwrap_or(0),
            &dockerfile,
            std::path::Path::new(sandbox_path),
        )
        .await
        {
            Some(crate::cache_brain::BuildUnit {
                unit_type: crate::cache_brain::BuildUnitType::DockerBuild {
                    stage: "build".into(),
                },
                input_signature: witness.key,
                environment_signature: env_string_or_default("DOCKER_DEFAULT_PLATFORM", ""),
                scope: format!("project:{}", project_id.unwrap_or(0)),
                trust_tier: crate::policy::TrustTier::Untrusted,
            })
        } else {
            None
        }
    } else if is_rust_build {
        let cargo_lock = std::path::Path::new(sandbox_path).join("Cargo.lock");
        if let Ok(witness) = crate::witness::WitnessBuilder::rust_build_witness(
            project_id.unwrap_or(0),
            &cargo_lock,
            "1.74.0",
            Some("cargo-witness"),
            "x86_64-unknown-linux-gnu",
            "release",
            "",
        )
        .await
        {
            Some(crate::cache_brain::BuildUnit {
                unit_type: crate::cache_brain::BuildUnitType::CargoBuild {
                    target: "x86_64-unknown-linux-gnu".into(),
                    profile: "release".into(),
                    features: "".into(),
                },
                input_signature: witness.key,
                environment_signature: env_string_or_default("RUSTFLAGS", ""),
                scope: format!("project:{}", project_id.unwrap_or(0)),
                trust_tier: crate::policy::TrustTier::Untrusted,
            })
        } else {
            None
        }
    } else {
        None
    };

    let Some(ref unit) = build_unit else {
        return Ok(None);
    };

    let verdict = cache_brain.plan_step(unit).await?;
    tracing::info!("CacheBrain Verdict: {:?}", verdict);

    let verdict_str = format!("{:?}", verdict);
    let reasons_str = match serde_json::to_string(&verdict) {
        Ok(s) => s,
        Err(_) => String::new(),
    };
    let _ = db
        .store_test_verdict(
            job_id,
            &unit.input_signature,
            &unit.input_signature,
            &unit.input_signature,
            &verdict_str,
            &format!("{:?}", unit.trust_tier),
            &reasons_str,
        )
        .await;

    if verdict.is_hit() {
        let cas_path = crate::config::data_dir()
            .join("cas")
            .join(&unit.input_signature);
        let manifest_path = cas_path.join("manifest.json");
        let payload_path = cas_path.join("payload.tar.zst");
        let manifest_exists = tokio::fs::try_exists(&manifest_path).await.unwrap_or(false);
        let payload_exists = tokio::fs::try_exists(&payload_path).await.unwrap_or(false);

        if manifest_exists && payload_exists {
            let extract_status = tokio::process::Command::new("tar")
                .arg("-I")
                .arg("zstd")
                .arg("-xf")
                .arg(&payload_path)
                .arg("-C")
                .arg(sandbox_path)
                .status()
                .await;

            match extract_status {
                Ok(s) if s.success() => {
                    tracing::info!(
                        "✅ Exact-Hit: extracted CAS payload {:?} into {}. Skipping execution.",
                        payload_path,
                        sandbox_path
                    );
                    std::process::exit(0);
                }
                Ok(s) => {
                    tracing::warn!(
                        "CAS extraction failed with exit code {:?}; falling back to cold execution.",
                        s.code()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "CAS extraction error: {:?}; falling back to cold execution.",
                        e
                    );
                }
            }
        } else {
            tracing::warn!(
                "Exact-hit verdict but CAS payload incomplete (manifest={}, payload={}); falling back to cold execution.",
                manifest_exists,
                payload_exists
            );
        }
    } else {
        tracing::info!("Cache Brain produced Miss/Deny verdict, falling to cold execution.");
    }

    Ok(build_unit)
}

/// Stores the build artifact to the action cache and CAS after a cold execution.
pub(super) async fn store_build_artifact(
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
