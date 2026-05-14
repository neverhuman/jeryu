use super::*;
use crate::state::{Db, ReleaseSecretSet, SecretAuditEvent};
use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};

pub async fn rotate_release_secrets(
    db: &Db,
    repo_root: &Path,
    repo_name: &str,
    version: &str,
    target: SecretTarget,
    deploy_env_path: &Path,
    runtime_env_path: &Path,
) -> Result<RotateSecretOutcome> {
    let (vault_env, rendered_deploy_env) =
        ensure_release_envs(repo_root, version, deploy_env_path).await?;
    let args = secret_rotation_args(
        "rotate-runtime-secrets",
        version,
        &rendered_deploy_env,
        runtime_env_path,
        target,
    );
    run_in_repo(repo_root, &args, "rotate-runtime-secrets").await?;

    let audit = audit_path(repo_root, version);
    let rendered_runtime_env = rendered_runtime_path(repo_root, version);
    let report = release_report_path(repo_root, version);
    let bundle = bundle_path(repo_root, version);
    let (runtime_secret_vault_path, recovery_password_vault_path, expires_at) =
        parse_audit_paths(&audit)?;

    if rendered_runtime_env.exists() {
        fs::copy(&rendered_runtime_env, runtime_env_path).with_context(|| {
            format!(
                "copy rotated runtime env {} -> {}",
                rendered_runtime_env.display(),
                runtime_env_path.display()
            )
        })?;
    }

    let now = Utc::now().to_rfc3339();
    db.upsert_release_secret_set(&ReleaseSecretSet {
        repo_name: repo_name.to_string(),
        version: version.to_string(),
        target: target.as_str().to_string(),
        authority_name: "local-vault".to_string(),
        status: "rotated".to_string(),
        rendered_deploy_env_path: rendered_deploy_env.display().to_string(),
        rendered_runtime_env_path: rendered_runtime_env.display().to_string(),
        audit_path: audit.display().to_string(),
        bundle_path: bundle.exists().then(|| bundle.display().to_string()),
        report_path: report.exists().then(|| report.display().to_string()),
        runtime_secret_vault_path: runtime_secret_vault_path.clone(),
        recovery_password_vault_path: recovery_password_vault_path.clone(),
        expires_at,
        rotated_at: now.clone(),
        finalized_at: None,
        updated_at: now.clone(),
    })
    .await?;
    db.insert_secret_audit_event(&SecretAuditEvent {
        id: None,
        repo_name: repo_name.to_string(),
        version: version.to_string(),
        target: target.as_str().to_string(),
        action: "rotate".to_string(),
        status: "ok".to_string(),
        detail: format!(
            "Vault {} {}",
            vault_env.addr,
            match runtime_secret_vault_path.clone() {
                Some(p) => p,
                None => String::new(),
            }
        ),
        created_at: now,
    })
    .await?;

    Ok(RotateSecretOutcome {
        repo_root: repo_root.display().to_string(),
        version: version.to_string(),
        target: target.as_str().to_string(),
        rendered_deploy_env: rendered_deploy_env.display().to_string(),
        rendered_runtime_env: rendered_runtime_env.display().to_string(),
        audit_path: audit.display().to_string(),
        bundle_path: bundle.exists().then(|| bundle.display().to_string()),
        report_path: report.exists().then(|| report.display().to_string()),
        runtime_secret_vault_path,
        recovery_password_vault_path,
    })
}

pub async fn finalize_release_secrets(
    db: &Db,
    repo_root: &Path,
    repo_name: &str,
    version: &str,
    target: SecretTarget,
    deploy_env_path: &Path,
    runtime_env_path: &Path,
) -> Result<PathBuf> {
    let (_, rendered_deploy_env) = ensure_release_envs(repo_root, version, deploy_env_path).await?;
    let args = secret_rotation_args(
        "finalize-secret-rotation",
        version,
        &rendered_deploy_env,
        runtime_env_path,
        target,
    );
    run_in_repo(repo_root, &args, "finalize-secret-rotation").await?;
    let rendered_runtime_env = rendered_runtime_path(repo_root, version);
    if rendered_runtime_env.exists() {
        fs::copy(&rendered_runtime_env, runtime_env_path).with_context(|| {
            format!(
                "copy finalized runtime env {} -> {}",
                rendered_runtime_env.display(),
                runtime_env_path.display()
            )
        })?;
    }
    let now = Utc::now().to_rfc3339();
    db.mark_release_secret_set_finalized(repo_name, version, target.as_str(), &now)
        .await?;
    db.insert_secret_audit_event(&SecretAuditEvent {
        id: None,
        repo_name: repo_name.to_string(),
        version: version.to_string(),
        target: target.as_str().to_string(),
        action: "finalize".to_string(),
        status: "ok".to_string(),
        detail: rendered_runtime_env.display().to_string(),
        created_at: now,
    })
    .await?;
    Ok(rendered_runtime_env)
}

pub async fn build_release_secret_report(
    db: &Db,
    repo_root: &Path,
    repo_name: &str,
    version: &str,
) -> Result<PathBuf> {
    let args = veox_command_args("build-release-handoff-report", version);
    run_in_repo(repo_root, &args, "build-release-handoff-report").await?;
    let report = release_report_path(repo_root, version);
    db.insert_secret_audit_event(&SecretAuditEvent {
        id: None,
        repo_name: repo_name.to_string(),
        version: version.to_string(),
        target: "shared".to_string(),
        action: "report".to_string(),
        status: "ok".to_string(),
        detail: report.display().to_string(),
        created_at: Utc::now().to_rfc3339(),
    })
    .await?;
    Ok(report)
}

pub async fn recover_release_secrets(
    db: &Db,
    repo_root: &Path,
    repo_name: &str,
    version: &str,
) -> Result<()> {
    let args = veox_command_args("print-release-recovery-instructions", version);
    run_in_repo(repo_root, &args, "print-release-recovery-instructions").await?;
    db.insert_secret_audit_event(&SecretAuditEvent {
        id: None,
        repo_name: repo_name.to_string(),
        version: version.to_string(),
        target: "shared".to_string(),
        action: "recover".to_string(),
        status: "ok".to_string(),
        detail: "printed recovery instructions".to_string(),
        created_at: Utc::now().to_rfc3339(),
    })
    .await?;
    Ok(())
}

pub fn default_release_paths() -> (PathBuf, PathBuf, PathBuf) {
    let root = crate::settings::release_repo_root();
    (
        root.clone(),
        root.join("env/prod.deploy.env"),
        root.join("env/prod.runtime.env"),
    )
}
