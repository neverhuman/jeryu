use anyhow::Result;
use std::env;
use tracing::info;

use super::support::{
    ensure_custom_executor_tools, env_bool_or_default, env_i64_or_default, env_string_or_default,
};

/// Handles `jeryu exec prepare`
/// Provisions the actual job container sandbox.
pub async fn run_prepare() -> Result<()> {
    let job_id = env_string_or_default("CUSTOM_ENV_CI_JOB_ID", "unknown");
    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");

    info!(
        job_id,
        project_dir, "Driver: preparing custom execution sandbox"
    );

    let sandbox_path = format!("{}-sandbox", project_dir);

    if super::support::fast_clone(&project_dir, &sandbox_path).is_err() {
        let _ = std::fs::create_dir_all(&sandbox_path);
    }

    crate::honeypot::seed_sandbox(&sandbox_path);

    Ok(())
}

/// Handles `jeryu exec run`
/// Executes a specific stage of the pipeline (step_script, build_script, etc.)
pub async fn run_stage(script_path: &str, stage: &str) -> Result<()> {
    let job_id = env_i64_or_default("CUSTOM_ENV_CI_JOB_ID", 0);
    let project_id_str = env_string_or_default("CUSTOM_ENV_CI_PROJECT_ID", "");
    let project_id = project_id_str.parse::<i64>().ok();

    super::validate_script_path(script_path)?;

    info!(job_id, stage, script_path, "Driver: running job stage");

    if stage == "build_script" {
        ensure_custom_executor_tools().await?;
    }

    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");
    let sandbox_path = format!("{}-sandbox", project_dir);

    let db = crate::state::Db::open().await?;
    let build_unit = super::stage_cache::resolve_build_unit(
        &db,
        job_id,
        project_id,
        stage,
        script_path,
        &sandbox_path,
    )
    .await?;

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
        let cargo_cache_enabled = env_bool_or_default("JERYU_CARGO_CACHE", true);
        let project_scope = match env::var("CUSTOM_ENV_CI_PROJECT_PATH_SLUG") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => match env::var("CUSTOM_ENV_CI_PROJECT_DIR") {
                Ok(project_dir) => {
                    match crate::cargo_cache::canonical_repo_key(std::path::Path::new(&project_dir))
                    {
                        Ok(value) if !value.trim().is_empty() => value,
                        _ => "unknown-project".to_string(),
                    }
                }
                Err(_) => "unknown-project".to_string(),
            },
        };
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
    let quarantine_marker = std::path::Path::new(&sandbox_path).join(".jeryu_quarantine");
    let is_quarantined = quarantine_marker.exists();

    if is_quarantined {
        let reason = std::fs::read_to_string(&quarantine_marker).unwrap_or_default();
        let log_snippet = String::from_utf8_lossy(&log_buffer_cloned.lock().unwrap()).to_string();
        let capsule = crate::capsule::FailureCapsule::capture(
            job_id,
            project_id.unwrap_or(0),
            stage,
            999,
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

    if let Some(ref unit) = build_unit {
        super::stage_cache::store_build_artifact(&db, job_id, project_id, unit, &sandbox_path)
            .await?;
    }

    Ok(())
}
