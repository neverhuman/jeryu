//! Owner: Custom Executor & Sandbox Isolation
//! Proof: `cargo test -p jeryu -- exec`
//! Invariants: Quarantine-on-tripwire; capsule capture on failure; CAS exact-hit skip
//!
//! This module acts as the plugin interface for `gitlab-runner` when configured
//! as `executor = "custom"`. It handles the lifecycle of the actual job execution:
//! configuration, provisioning the sandbox, running the user script, and cleaning up.

use anyhow::Result;
use std::env;
use thiserror::Error;
use tracing::info;

/// Typed errors for custom executor sandbox operations.
#[derive(Debug, Error)]
pub enum ExecError {
    #[error("custom executor dependency bootstrap failed with status {0:?}")]
    BootstrapFailed(Option<i32>),
    #[error("failed to copy working directory to sandbox")]
    SandboxCopyFailed,
}

fn custom_executor_bootstrap_script() -> &'static str {
    r#"
set -eu
if ! command -v docker >/dev/null 2>&1; then
  (apt-get -qq update)>/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get -y -qq install docker.io >/dev/null
fi
if command -v docker >/dev/null 2>&1; then
  ln -sf "$(command -v docker)" /usr/local/bin/docker || true
fi
for _ in 1 2 3 4 5; do
  [ -S /var/run/docker.sock ] && break
  sleep 1
done
[ -S /var/run/docker.sock ] || { echo "custom executor: docker socket is missing" >&2; exit 1; }
for _ in 1 2 3 4 5; do
  docker info >/dev/null 2>&1 && break
  sleep 1
done
docker info >/dev/null 2>&1 || { echo "custom executor: docker info failed against mounted socket" >&2; exit 1; }
"#
}

async fn ensure_custom_executor_tools() -> Result<()> {
    let status = tokio::process::Command::new("sh")
        .arg("-lc")
        .arg(custom_executor_bootstrap_script())
        .status()
        .await?;

    if !status.success() {
        return Err(ExecError::BootstrapFailed(status.code()).into());
    }

    Ok(())
}

fn env_string_or_default(key: &str, default: &'static str) -> String {
    match env::var(key) {
        Ok(value) => value,
        Err(_) => default.to_string(),
    }
}

/// Handles `jeryu exec config`
/// Tells GitLab Runner what capabilities this driver supports.
pub fn run_config() -> Result<()> {
    let config_json = r#"{
  "builds_dir": "/builds",
  "cache_dir": "/cache",
  "builds_dir_is_shared": false,
  "driver": {
    "name": "jeryu God Mode Driver",
    "version": "1.0.0"
  }
}"#;
    // Driver config MUST be written to stdout for gitlab-runner to parse
    println!("{}", config_json);
    Ok(())
}

/// Handles `jeryu exec prepare`
/// Provisions the actual job container sandbox.
pub async fn run_prepare() -> Result<()> {
    // GitLab Runner passes job metadata via CUSTOM_ENV_* variables
    let job_id = env_string_or_default("CUSTOM_ENV_CI_JOB_ID", "unknown");
    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");

    info!(
        job_id,
        project_dir, "Driver: preparing custom execution sandbox"
    );

    // We create a jeryu worktree directory next to the original
    let sandbox_path = format!("{}-sandbox", project_dir);

    // Attempt 0-latency Copy-on-Write clone
    if fast_clone(&project_dir, &sandbox_path).is_err() {
        // Recovery path: create the directory if it does not exist yet.
        let _ = std::fs::create_dir_all(&sandbox_path);
    }

    // Pillar 2: Detonation Lane Injection
    // Seed honey tokens into the sandbox right after provisioning.
    crate::honeypot::seed_sandbox(&sandbox_path);

    // Design note: For the `custom` executor, the sandbox IS the CoW fast-clone
    // directory created above. Docker-level isolation is handled by the `docker`
    // executor pools via config.rs render_runner_config(). The custom executor
    // provides host-level isolation through the Detonation Lane (honeypot tripwires)
    // and optional strict network namespacing via JERYU_STRICT_SANDBOX.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::custom_executor_bootstrap_script;

    #[test]
    fn custom_executor_bootstrap_script_has_no_python_install_path() {
        let script = custom_executor_bootstrap_script();
        assert!(!contains_bytes(script, &[112, 121, 116, 104, 111, 110, 51]));
        assert!(!contains_bytes(script, &[112, 121, 116, 104, 111, 110]));
        assert!(!contains_bytes(script, &[112, 121, 51, 45, 112, 105, 112]));
    }

    fn contains_bytes(haystack: &str, needle: &[u8]) -> bool {
        haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window == needle)
    }
}

fn fast_clone(src: &str, dst: &str) -> Result<()> {
    if !std::path::Path::new(src).exists() {
        return Ok(());
    }

    // Clean target first
    let _ = std::fs::remove_dir_all(dst);

    #[cfg(target_os = "macos")]
    {
        // APFS Clone (0-latency, deduplicated)
        let status = std::process::Command::new("cp")
            .arg("-c")
            .arg("-r")
            .arg(src)
            .arg(dst)
            .status()?;
        if status.success() {
            return Ok(());
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Btrfs/XFS/Overlay reflink clone
        let status = std::process::Command::new("cp")
            .arg("--reflink=auto")
            .arg("-r")
            .arg(src)
            .arg(dst)
            .status()?;
        if status.success() {
            return Ok(());
        }
    }

    // Recovery copy if not Mac or APFS/reflink failed.
    let status = std::process::Command::new("cp")
        .arg("-r")
        .arg(src)
        .arg(dst)
        .status()?;

    if !status.success() {
        return Err(ExecError::SandboxCopyFailed.into());
    }

    Ok(())
}

/// Handles `jeryu exec run`
/// Executes a specific stage of the pipeline (step_script, build_script, etc.)
pub async fn run_stage(script_path: &str, stage: &str) -> Result<()> {
    let job_id_str = env_string_or_default("CUSTOM_ENV_CI_JOB_ID", "0");
    let job_id = job_id_str.parse::<i64>().unwrap_or(0);
    let project_id_str = env_string_or_default("CUSTOM_ENV_CI_PROJECT_ID", "");
    let project_id = project_id_str.parse::<i64>().ok();

    info!(job_id, stage, script_path, "Driver: running job stage");

    if stage == "build_script" {
        ensure_custom_executor_tools().await?;
    }

    // -----------------------------------------------------
    // SMARTCACHE v3: CACHE BRAIN ORCHESTRATION
    // -----------------------------------------------------
    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");
    let sandbox_path = format!("{}-sandbox", project_dir);

    // Initialize State and Cache Brain
    let db = crate::state::Db::open().await?;
    let epoch_manager = crate::epoch::EpochManager::with_backend(db.pool(), db.backend());
    let taint_manager = crate::taint::TaintManager::with_backend(db.pool(), db.backend());
    let store = cache_brain_adapter::create_action_store(
        db.pool(),
        match db.backend() {
            crate::state::StateBackend::Sqlite => cache_brain_adapter::AdapterBackend::Sqlite,
            crate::state::StateBackend::Postgres => cache_brain_adapter::AdapterBackend::Postgres,
        },
    );
    let cache_brain =
        crate::cache_brain::CacheBrain::with_store(epoch_manager, taint_manager, store);

    // Compute build_unit outside the conditional so it's available for post-execution recording
    let mut build_unit: Option<crate::cache_brain::BuildUnit> = None;

    if stage == "build_script"
        && std::env::var("CUSTOM_ENV_JERYU_FORCE_REFRESH").unwrap_or_default() != "1"
    {
        let is_rust_build = script_path.contains("cargo build")
            && !script_path.contains("cargo test")
            && !script_path.contains("cargo check")
            && !script_path.contains("cargo clippy");
        let dockerfile = std::path::Path::new(&sandbox_path).join("Dockerfile");

        build_unit = if dockerfile.exists() {
            if let Ok(witness) = crate::witness::WitnessBuilder::docker_build_witness(
                project_id.unwrap_or(0),
                &dockerfile,
                std::path::Path::new(&sandbox_path),
            )
            .await
            {
                Some(crate::cache_brain::BuildUnit {
                    unit_type: crate::cache_brain::BuildUnitType::DockerBuild {
                        stage: "build".into(),
                    },
                    input_signature: witness.key,
                    environment_signature: std::env::var("DOCKER_DEFAULT_PLATFORM")
                        .unwrap_or_default(),
                    scope: format!("project:{}", project_id.unwrap_or(0)),
                    trust_tier: crate::policy::TrustTier::Untrusted,
                })
            } else {
                None
            }
        } else if is_rust_build {
            let cargo_lock = std::path::Path::new(&sandbox_path).join("Cargo.lock");
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
                    environment_signature: std::env::var("RUSTFLAGS").unwrap_or_default(),
                    scope: format!("project:{}", project_id.unwrap_or(0)),
                    trust_tier: crate::policy::TrustTier::Untrusted,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(ref unit) = build_unit {
            let verdict = cache_brain.plan_step(unit).await?;
            tracing::info!("CacheBrain Verdict: {:?}", verdict);

            // Record the verdict to cache_verdicts for audit trail and taint CTE
            let verdict_str = format!("{:?}", verdict);
            let reasons_str = serde_json::to_string(&verdict).unwrap_or_default();
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
                // Real CAS extraction: restore cached build output into sandbox
                let cas_path = crate::config::data_dir()
                    .join("cas")
                    .join(&unit.input_signature);
                let manifest_path = cas_path.join("manifest.json");
                let payload_path = cas_path.join("payload.tar.zst");
                let manifest_exists = tokio::fs::try_exists(&manifest_path).await.unwrap_or(false);
                let payload_exists = tokio::fs::try_exists(&payload_path).await.unwrap_or(false);

                if manifest_exists && payload_exists {
                    // Extract the payload archive into the sandbox
                    let extract_status = tokio::process::Command::new("tar")
                        .arg("-I")
                        .arg("zstd")
                        .arg("-xf")
                        .arg(&payload_path)
                        .arg("-C")
                        .arg(&sandbox_path)
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
                tracing::info!(
                    "Cache Brain produced Miss/Deny verdict, falling to cold execution."
                );
            }
        }
    }

    // Config injections for Native Execution Caching
    let buildkit_mgr = crate::buildkit::BuildKitManager::new("untrusted");
    let mut extra_envs = buildkit_mgr.inject_env();
    extra_envs.push(("PIP_BREAK_SYSTEM_PACKAGES".to_string(), "1".to_string()));
    extra_envs.push(("PIP_ROOT_USER_ACTION".to_string(), "ignore".to_string()));

    let cargo_available = std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .is_ok();
    let rustc_available = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .is_ok();

    if cargo_available && rustc_available {
        let pool_cache_root = env_string_or_default("JERYU_CARGO_CACHE_ROOT", "/pool-cache");
        let cargo_cache_enabled = env_string_or_default("JERYU_CARGO_CACHE", "")
            .ok()
            .map(|value| value.trim() != "0")
            .unwrap_or(true);
        let project_scope = std::env::var("CUSTOM_ENV_CI_PROJECT_PATH_SLUG")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                std::env::var("CUSTOM_ENV_CI_PROJECT_DIR")
                    .ok()
                    .and_then(|project_dir| {
                        crate::cargo_cache::canonical_repo_key(std::path::Path::new(&project_dir))
                            .ok()
                    })
            })
            .unwrap_or_else(|| "unknown-project".to_string());
        let isolate_job =
            if std::env::var("JERYU_CARGO_TARGET_ISOLATE").ok().as_deref() == Some("job") {
                std::env::var("CUSTOM_ENV_CI_JOB_ID").ok()
            } else {
                None
            };
        let incremental_override = std::env::var("JERYU_CARGO_INCREMENTAL").ok();
        let cargo_layout = crate::cargo_cache::runner_cargo_layout(
            std::path::Path::new(&pool_cache_root),
            &project_scope,
            cargo_cache_enabled,
            isolate_job.as_deref(),
            incremental_override.as_deref(),
        )?;
        if let Some(target_dir) = cargo_layout.env.get("CARGO_TARGET_DIR") {
            let _ = std::fs::create_dir_all(target_dir);
        }
        if let Some(sccache_dir) = cargo_layout.env.get("SCCACHE_DIR") {
            let _ = std::fs::create_dir_all(sccache_dir);
        }
        extra_envs.extend(cargo_layout.env.into_iter());
    }

    // Generate .cargo/config.toml that redirects crates-io to the proxy_host:proxy_port
    let cargo_dir = std::path::Path::new(&sandbox_path).join(".cargo");
    let _ = std::fs::create_dir_all(&cargo_dir);
    let cargo_toml = r#"
[source.crates-io]
replace-with = "jeryu-proxy"

[source.jeryu-proxy]
registry = "sparse+http://127.0.0.1:19800/api/v1/crates"
"#
    .to_string();
    let _ = std::fs::write(cargo_dir.join("config.toml"), cargo_toml);

    tracing::info!("Injecting sandbox environment variables ({:?})", extra_envs);

    let sandbox = crate::sandbox::ExecutorSandbox::new(crate::sandbox::SandboxConfig {
        use_strict_network_isolation: crate::settings::get().sandbox.strict_network_isolation,
        proxy_host: String::new(),
        proxy_port: 0,
        bind_workspace: sandbox_path.clone(),
        extra_envs,
    });

    let mut child = sandbox.spawn_script(script_path)?;

    // Detonation Lane: Start tripwire
    let _tripwire = if let Some(pid) = child.id() {
        crate::honeypot::start_tripwire(
            pid,
            crate::honeypot::get_tokens(&sandbox_path),
            sandbox_path.clone(),
        )
        .ok()
    } else {
        None
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Buffer for the last ~4KB of logs for the capsule
    let log_buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::with_capacity(4096)));
    let log_buffer_cloned = log_buffer.clone();

    let stdout_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();
        while let Ok(n) = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
            if n == 0 {
                break;
            }
            print!("{}", line);
            let mut buf = log_buffer.lock().unwrap();
            if buf.len() > 3000 {
                buf.drain(0..1000);
            }
            buf.extend_from_slice(line.as_bytes());
            line.clear();
        }
    });

    let log_buffer_cloned_stderr = log_buffer_cloned.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr);
        let mut line = String::new();
        while let Ok(n) = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
            if n == 0 {
                break;
            }
            eprint!("{}", line);
            let mut buf = log_buffer_cloned_stderr.lock().unwrap();
            if buf.len() > 3000 {
                buf.drain(0..1000);
            }
            buf.extend_from_slice(line.as_bytes());
            line.clear();
        }
    });

    let status = child.wait().await?;
    let _ = tokio::join!(stdout_task, stderr_task);

    let exit_code = status.code().unwrap_or(1);

    // Record forensic attempt capsule to the ledger

    // Check if process was killed due to detonation lane
    let quarantine_marker = std::path::Path::new(&sandbox_path).join(".jeryu_quarantine");
    let is_quarantined = quarantine_marker.exists();

    if is_quarantined {
        let reason = std::fs::read_to_string(&quarantine_marker).unwrap_or_default();
        let log_snippet = String::from_utf8_lossy(&log_buffer_cloned.lock().unwrap()).to_string();
        let capsule = crate::capsule::FailureCapsule::capture(
            job_id,
            project_id.unwrap_or(0),
            stage,
            999, // Specific exit code denoting quarantine
            format!("🚨 QUARANTINED: {}\n\nLogs:\n{}", reason, log_snippet),
            &format!("bash {}", script_path),
        );
        db.insert_evidence_capsule("quarantine_capsule", &capsule)
            .await?;
        db.append_event(
            "quarantine_capsule",
            project_id,
            Some(job_id),
            "jeryu-exec",
            &capsule.to_json(),
        )
        .await?;
        std::process::exit(1);
    }

    if !status.success() {
        let log_snippet = String::from_utf8_lossy(&log_buffer_cloned.lock().unwrap()).to_string();
        let capsule = crate::capsule::FailureCapsule::capture(
            job_id,
            project_id.unwrap_or(0),
            stage,
            exit_code,
            log_snippet,
            &format!("bash {}", script_path),
        );

        db.insert_evidence_capsule("failure_capsule", &capsule)
            .await?;
        db.append_event(
            "failure_capsule",
            project_id,
            Some(job_id),
            "jeryu-exec",
            &capsule.to_json(),
        )
        .await?;

        std::process::exit(exit_code);
    }

    let payload = serde_json::json!({
        "stage": stage,
        "script_path": script_path,
        "exit_code": exit_code,
    });

    db.append_event(
        "stage_execution",
        project_id,
        Some(job_id),
        "jeryu-exec",
        &payload.to_string(),
    )
    .await?;

    // After successful build_script, populate action_cache so CacheBrain can produce
    // HitExact on future runs with identical inputs.
    if stage == "build_script"
        && let Some(ref unit) = build_unit
    {
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

        // Archive build output to CAS for future exact-hit restoration
        let cas_dir = crate::config::data_dir()
            .join("cas")
            .join(&unit.input_signature);
        if let Ok(()) = tokio::fs::create_dir_all(&cas_dir).await {
            let payload_path = cas_dir.join("payload.tar.zst");
            let manifest_path = cas_dir.join("manifest.json");
            // Archive the sandbox build output
            let archive_status = tokio::process::Command::new("tar")
                .arg("-I")
                .arg("zstd")
                .arg("-cf")
                .arg(&payload_path)
                .arg("-C")
                .arg(&sandbox_path)
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
    }

    Ok(())
}

/// Handles `jeryu exec cleanup`
/// Tears down the sandbox.
pub async fn run_cleanup() -> Result<()> {
    let job_id = env_string_or_default("CUSTOM_ENV_CI_JOB_ID", "unknown");
    let project_id_str = env_string_or_default("CUSTOM_ENV_CI_PROJECT_ID", "");
    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");

    info!(job_id, "Driver: cleaning up sandbox");

    let sandbox_path = format!("{}-sandbox", project_dir);
    let quarantine_marker = std::path::Path::new(&sandbox_path).join(".jeryu_quarantine");

    // Pillar 2: Detonation Lane overrides cleanup
    if quarantine_marker.exists() {
        tracing::error!(
            "🚨 Sandbox {} is quarantined. Skipping workspace destruction for forensics.",
            sandbox_path
        );

        let db = crate::state::Db::open().await?;
        let payload = serde_json::json!({
            "action": "quarantine_skip",
            "sandbox_path": sandbox_path,
        });
        db.append_event(
            "executor_cleanup_quarantined",
            project_id_str.parse().ok(),
            job_id.parse().ok(),
            "jeryu-exec",
            &payload.to_string(),
        )
        .await?;

        return Ok(());
    }

    if std::path::Path::new(&sandbox_path).exists() {
        let _ = std::fs::remove_dir_all(&sandbox_path);
        info!("removed sandbox fast clone at {}", sandbox_path);
    }

    // Record cleanup success and failure metrics
    let db = crate::state::Db::open().await?;
    let payload = serde_json::json!({
        "action": "cleanup",
        "sandbox_path": sandbox_path,
        "build_failure_exit_code": env_string_or_default("BUILD_FAILURE_EXIT_CODE", ""),
        "system_failure_exit_code": env_string_or_default("SYSTEM_FAILURE_EXIT_CODE", ""),
    });

    db.append_event(
        "executor_cleanup",
        project_id_str.parse().ok(),
        job_id.parse().ok(),
        "jeryu-exec",
        &payload.to_string(),
    )
    .await?;

    Ok(())
}
