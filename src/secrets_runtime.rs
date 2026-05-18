use super::*;
use crate::docker::DockerCtl;
use crate::state::{Db, SecretAuthority};
use anyhow::{Context, Result, anyhow};
use chrono::{Duration, Utc};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::process::Command;

async fn build_operational_env() -> Result<VaultEnv> {
    ensure_vault_files().await?;
    let root_password = ensure_jeryu_compose_inputs()?;
    write_restricted(
        &config::compose_file(),
        &config::render_compose(&root_password),
    )?;
    DockerCtl::connect()?.compose_up_service("vault").await?;

    let client = Client::builder().build().context("build HTTP client")?;
    let addr = format!("http://127.0.0.1:{}", config::VAULT_HTTP_PORT);
    wait_for_vault_http(&client, &addr).await?;

    let mut env = load_vault_env()?.unwrap_or(VaultEnv {
        addr: addr.clone(),
        token: String::new(),
        mount: config::VAULT_DEFAULT_MOUNT.to_string(),
        prefix: config::VAULT_DEFAULT_PREFIX.to_string(),
    });

    let mut health = fetch_vault_health(&client, &addr).await?;
    let mut bootstrap = load_bootstrap_material()?;
    if !health.initialized {
        let material = initialize_vault(&client, &addr).await?;
        save_bootstrap_material(&material)?;
        bootstrap = Some(material);
        health = fetch_vault_health(&client, &addr).await?;
    }

    if health.sealed {
        let Some(bootstrap) = bootstrap.as_ref() else {
            return Err(anyhow!("Vault is sealed but bootstrap material is missing"));
        };
        let Some(key) = bootstrap.unseal_keys_b64.first() else {
            return Err(anyhow!("Vault bootstrap material is missing unseal keys"));
        };
        unseal_vault(&client, &addr, key).await?;
    }

    let Some(bootstrap) = load_bootstrap_material()? else {
        return Err(anyhow!(
            "Vault bootstrap material missing after initialization"
        ));
    };
    ensure_kv_v2_mount(&client, &env.addr, &bootstrap.root_token, &env.mount).await?;
    write_policy(&client, &env, &bootstrap.root_token).await?;
    if env.token.is_empty() || !token_is_usable(&client, &env).await? {
        env.token = bootstrap.root_token.clone();
        env.token = create_ops_token(&client, &env, &bootstrap.root_token).await?;
        save_vault_env(&env)?;
    } else {
        env.addr = addr;
        save_vault_env(&env)?;
    }
    Ok(env)
}

fn redacted_fingerprint(value: &str) -> String {
    let prefix: String = value.chars().take(6).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

async fn record_vault_authority(
    db: Option<&Db>,
    env: &VaultEnv,
    health: &VaultHealthResponse,
) -> Result<()> {
    if let Some(db) = db {
        db.upsert_secret_authority(&SecretAuthority {
            name: "local-vault".to_string(),
            kind: "vault".to_string(),
            address: env.addr.clone(),
            status: if health.sealed { "sealed" } else { "ready" }.to_string(),
            mount: env.mount.clone(),
            prefix: env.prefix.clone(),
            token_fingerprint: redacted_fingerprint(&env.token),
            metadata_path: config::vault_env_file().display().to_string(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .await?;
    }
    Ok(())
}

fn build_vault_status_report(env: VaultEnv, health: VaultHealthResponse) -> VaultStatusReport {
    VaultStatusReport {
        addr: env.addr,
        initialized: health.initialized,
        sealed: health.sealed,
        healthy: health.initialized && !health.sealed,
        token_present: !env.token.is_empty(),
        mount: env.mount,
        prefix: env.prefix,
        bootstrap_file: config::vault_bootstrap_file().display().to_string(),
        env_file: config::vault_env_file().display().to_string(),
    }
}

fn veox_command_args(command: &str, version: &str) -> Vec<String> {
    vec![
        "run".to_string(),
        "-p".to_string(),
        "veox-deploy".to_string(),
        "--".to_string(),
        command.to_string(),
        "--version".to_string(),
        version.to_string(),
    ]
}

fn secret_rotation_args(
    action: &str,
    version: &str,
    deploy_env: &Path,
    runtime_env: &Path,
    target: SecretTarget,
) -> Vec<String> {
    let mut args = veox_command_args(action, version);
    args.push("--deploy-env".to_string());
    args.push(deploy_env.display().to_string());
    args.push("--runtime-env".to_string());
    args.push(runtime_env.display().to_string());
    args.push("--target".to_string());
    args.push(target.as_str().to_string());
    args
}

pub async fn run_secrets_init(db: Option<&Db>) -> Result<VaultStatusReport> {
    vault_status(db).await
}

pub async fn vault_status(db: Option<&Db>) -> Result<VaultStatusReport> {
    let env = build_operational_env().await?;
    let client = Client::builder().build().context("build HTTP client")?;
    let health = fetch_vault_health(&client, &env.addr).await?;
    record_vault_authority(db, &env, &health).await?;
    Ok(build_vault_status_report(env, health))
}

async fn ensure_release_envs(
    repo_root: &Path,
    version: &str,
    deploy_env_path: &Path,
) -> Result<(VaultEnv, PathBuf)> {
    let env = build_operational_env().await?;
    let mut deploy_env = parse_env_file(deploy_env_path)?;
    deploy_env.insert("NHT_VAULT_ADDR".to_string(), env.addr.clone());
    deploy_env.insert("NHT_VAULT_TOKEN".to_string(), env.token.clone());
    deploy_env.insert("NHT_VAULT_MOUNT".to_string(), env.mount.clone());
    deploy_env.insert("NHT_VAULT_PREFIX".to_string(), env.prefix.clone());
    deploy_env.insert(
        "NHT_DEPLOY_CREDENTIAL_LEASE_ID".to_string(),
        format!("jeryu-deploy-lease-{}", random_alnum(16)),
    );
    deploy_env.insert(
        "NHT_DEPLOY_CREDENTIAL_EXPIRES_AT".to_string(),
        (Utc::now() + Duration::hours(24)).to_rfc3339(),
    );
    let rendered_deploy_env = repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/jeryu.prod.deploy.env");
    write_env_map(&rendered_deploy_env, &deploy_env)?;
    Ok((env, rendered_deploy_env))
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

// ---------------------------------------------------------------------------
// Release secret operations (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "secrets_runtime_ops.rs"]
mod secrets_runtime_ops;
pub use secrets_runtime_ops::*;
