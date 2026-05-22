use super::*;
use crate::state::Db;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "secrets_runtime_env.rs"]
mod runtime_env;
use self::runtime_env::{ensure_release_envs, secret_rotation_args, veox_command_args};

pub async fn vault_status_observed(db: Option<&Db>) -> Result<runtime_env::VaultStatusObservation> {
    runtime_env::vault_status_observed(db).await
}

pub async fn run_secrets_init(db: Option<&Db>) -> Result<VaultStatusReport> {
    runtime_env::run_secrets_init(db).await
}

pub async fn vault_status(db: Option<&Db>) -> Result<VaultStatusReport> {
    runtime_env::vault_status(db).await
}

pub async fn run_secrets_provision(db: Option<&Db>) -> Result<VaultStatusReport> {
    runtime_env::run_secrets_provision(db).await
}

pub(crate) async fn run_in_repo(repo_root: &Path, args: &[String], label: &str) -> Result<()> {
    let status = Command::new("cargo")
        .current_dir(repo_root)
        .args(args)
        .status()
        .with_context(|| format!("run {label}"))?;
    if !status.success() {
        return Err(SecretError::CommandFailed(label.to_string(), status.code()).into());
    }
    Ok(())
}

pub(crate) fn audit_path(repo_root: &Path, version: &str) -> PathBuf {
    repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/secret-rotation-audit.json")
}

pub(crate) fn rendered_runtime_path(repo_root: &Path, version: &str) -> PathBuf {
    repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/prod.runtime.env")
}

pub(crate) fn release_report_path(repo_root: &Path, version: &str) -> PathBuf {
    repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/release-handoff.pdf")
}

pub(crate) fn bundle_path(repo_root: &Path, version: &str) -> PathBuf {
    repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/release-secrets.enc")
}

pub(crate) fn parse_audit_paths(
    path: &Path,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).with_context(|| format!("decode {}", path.display()))?;
    Ok((
        value
            .pointer("/runtime_secrets/vault_runtime_secret_path")
            .and_then(|item| item.as_str())
            .map(ToOwned::to_owned),
        value
            .pointer("/runtime_secrets/vault_recovery_password_path")
            .and_then(|item| item.as_str())
            .map(ToOwned::to_owned),
        value
            .pointer("/runtime_secrets/db_expires_at")
            .and_then(|item| item.as_str())
            .map(ToOwned::to_owned),
    ))
}

#[path = "secrets_runtime_ops.rs"]
mod secrets_runtime_ops;
pub use secrets_runtime_ops::*;
