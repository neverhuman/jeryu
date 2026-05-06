//! Owner: Secrets & Vault Lifecycle
//! Proof: `cargo test -p jeryu -- secrets`
//! Invariants: Rotation is current/previous pair; never raw plaintext; 0600 perms on all secret files

use anyhow::{Context, Result, anyhow};
use chrono::{Duration, Utc};
use rand::Rng;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::process::Command;

use crate::config;
use crate::docker::DockerCtl;
use crate::state::{Db, ReleaseSecretSet, SecretAuditEvent, SecretAuthority};

/// Typed errors for Vault secrets lifecycle.
#[derive(Debug, Error)]
pub enum SecretError {
    #[error("unknown secret target: {0}")]
    UnknownTarget(String),
    #[error("Vault did not become reachable at {0}")]
    VaultUnreachable(String),
    #[error("unexpected Vault health status: {0}")]
    VaultUnexpectedStatus(reqwest::StatusCode),
    #[error("Vault init failed: {0}")]
    VaultInitFailed(reqwest::StatusCode),
    #[error("Vault unseal failed: {0}")]
    VaultUnsealFailed(reqwest::StatusCode),
    #[error("Vault mount `{0}` exists but is not kv-v2")]
    VaultMountNotKvV2(String),
    #[error("Vault mount creation failed: {0}")]
    VaultMountCreationFailed(reqwest::StatusCode),
    #[error("writing Vault policy failed: {0}")]
    VaultPolicyFailed(reqwest::StatusCode),
    #[error("creating Vault ops token failed: {0}")]
    VaultTokenCreationFailed(reqwest::StatusCode),
    #[error("{0} failed with exit code {1:?}")]
    CommandFailed(String, Option<i32>),
}

const DEFAULT_REPO_ROOT: &str = "/home/ubuntu/dougx";
const OPS_POLICY_NAME: &str = "jeryu-release-ops";
const OPS_DISPLAY_NAME: &str = "jeryu-release-control-plane";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecretTarget {
    Canary,
    Prod,
}

impl SecretTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Canary => "canary",
            Self::Prod => "prod",
        }
    }
}

impl std::str::FromStr for SecretTarget {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "canary" => Ok(Self::Canary),
            "prod" | "production" => Ok(Self::Prod),
            other => Err(SecretError::UnknownTarget(other.to_string()).into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultBootstrapMaterial {
    root_token: String,
    unseal_keys_b64: Vec<String>,
    initialized_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultEnv {
    addr: String,
    token: String,
    mount: String,
    prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatusReport {
    pub addr: String,
    pub initialized: bool,
    pub sealed: bool,
    pub healthy: bool,
    pub token_present: bool,
    pub mount: String,
    pub prefix: String,
    pub bootstrap_file: String,
    pub env_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateSecretOutcome {
    pub repo_root: String,
    pub version: String,
    pub target: String,
    pub rendered_deploy_env: String,
    pub rendered_runtime_env: String,
    pub audit_path: String,
    pub bundle_path: Option<String>,
    pub report_path: Option<String>,
    pub runtime_secret_vault_path: Option<String>,
    pub recovery_password_vault_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VaultHealthResponse {
    initialized: bool,
    sealed: bool,
}

#[derive(Debug, Deserialize)]
struct VaultInitResponse {
    root_token: String,
    #[serde(default)]
    unseal_keys_b64: Vec<String>,
    #[serde(default)]
    keys_base64: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct VaultTokenCreateResponse {
    auth: VaultAuth,
}

#[derive(Debug, Deserialize)]
struct VaultAuth {
    client_token: String,
}

fn repo_root() -> PathBuf {
    std::env::var("JERYU_RELEASE_REPO_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            crate::settings::get()
                .release
                .repo_root
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REPO_ROOT))
}

fn random_alnum(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..ALPHABET.len());
            ALPHABET[idx] as char
        })
        .collect()
}

fn write_restricted(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .with_context(|| format!("metadata {}", path.display()))?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms).with_context(|| format!("chmod {}", path.display()))?;
    }
    Ok(())
}

fn parse_env_file(path: &Path) -> Result<BTreeMap<String, String>> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(parse_env_str(&raw))
}

fn parse_env_str(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        out.insert(
            key.trim().to_string(),
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string(),
        );
    }
    out
}

fn write_env_map(path: &Path, values: &BTreeMap<String, String>) -> Result<()> {
    let body = values
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n");
    write_restricted(path, &format!("{body}\n"))
}

async fn ensure_vault_files() -> Result<()> {
    fs::create_dir_all(config::vault_storage_dir())
        .with_context(|| format!("create {}", config::vault_storage_dir().display()))?;
    Ok(())
}

fn ensure_jeryu_compose_inputs() -> Result<String> {
    let env_path = config::env_file();
    if !env_path.exists() {
        let root_password = random_alnum(24);
        let webhook_secret = random_alnum(32);
        let body = format!(
            "# Generated by jeryu secrets init — do not share\n\
             GITLAB_ROOT_PASSWORD={root_password}\n\
             JERYU_WEBHOOK_SECRET={webhook_secret}\n"
        );
        write_restricted(&env_path, &body)?;
        return Ok(root_password);
    }
    let parsed = parse_env_file(&env_path)?;
    if let Some(root_password) = parsed.get("GITLAB_ROOT_PASSWORD").cloned() {
        return Ok(root_password);
    }
    let root_password = random_alnum(24);
    let webhook_secret = parsed
        .get("JERYU_WEBHOOK_SECRET")
        .cloned()
        .unwrap_or_else(|| random_alnum(32));
    let body = format!(
        "# Generated by jeryu secrets init — do not share\n\
         GITLAB_ROOT_PASSWORD={root_password}\n\
         JERYU_WEBHOOK_SECRET={webhook_secret}\n"
    );
    write_restricted(&env_path, &body)?;
    Ok(root_password)
}

async fn wait_for_vault_http(client: &Client, addr: &str) -> Result<()> {
    let url = format!("{}/v1/sys/health", addr.trim_end_matches('/'));
    for _ in 0..60 {
        match client.get(&url).send().await {
            Ok(_) => return Ok(()),
            Err(_) => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
        }
    }
    Err(SecretError::VaultUnreachable(addr.to_string()).into())
}

async fn fetch_vault_health(client: &Client, addr: &str) -> Result<VaultHealthResponse> {
    let url = format!("{}/v1/sys/health", addr.trim_end_matches('/'));
    let response = client.get(url).send().await.context("query Vault health")?;
    match response.status() {
        StatusCode::OK
        | StatusCode::TOO_MANY_REQUESTS
        | StatusCode::BAD_REQUEST
        | StatusCode::NOT_IMPLEMENTED
        | StatusCode::SERVICE_UNAVAILABLE => response
            .json()
            .await
            .context("decode Vault health response"),
        status => Err(SecretError::VaultUnexpectedStatus(status).into()),
    }
}

fn load_bootstrap_material() -> Result<Option<VaultBootstrapMaterial>> {
    let path = config::vault_bootstrap_file();
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(
        serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("read {}", path.display()))?,
        )
        .with_context(|| format!("decode {}", path.display()))?,
    ))
}

fn save_bootstrap_material(material: &VaultBootstrapMaterial) -> Result<()> {
    let payload = serde_json::to_string_pretty(material)?;
    write_restricted(&config::vault_bootstrap_file(), &payload)
}

fn load_vault_env() -> Result<Option<VaultEnv>> {
    let path = config::vault_env_file();
    if !path.exists() {
        return Ok(None);
    }
    let parsed = parse_env_file(&path)?;
    let Some(addr) = parsed.get("JERYU_VAULT_ADDR").cloned() else {
        return Ok(None);
    };
    let Some(token) = parsed.get("JERYU_VAULT_TOKEN").cloned() else {
        return Ok(None);
    };
    Ok(Some(VaultEnv {
        addr,
        token,
        mount: parsed
            .get("JERYU_VAULT_MOUNT")
            .cloned()
            .unwrap_or_else(|| config::VAULT_DEFAULT_MOUNT.to_string()),
        prefix: parsed
            .get("JERYU_VAULT_PREFIX")
            .cloned()
            .unwrap_or_else(|| config::VAULT_DEFAULT_PREFIX.to_string()),
    }))
}

fn save_vault_env(env: &VaultEnv) -> Result<()> {
    let mut values = BTreeMap::new();
    values.insert("JERYU_VAULT_ADDR".to_string(), env.addr.clone());
    values.insert("JERYU_VAULT_TOKEN".to_string(), env.token.clone());
    values.insert("JERYU_VAULT_MOUNT".to_string(), env.mount.clone());
    values.insert("JERYU_VAULT_PREFIX".to_string(), env.prefix.clone());
    write_env_map(&config::vault_env_file(), &values)
}

async fn initialize_vault(client: &Client, addr: &str) -> Result<VaultBootstrapMaterial> {
    let url = format!("{}/v1/sys/init", addr.trim_end_matches('/'));
    let response = client
        .put(url)
        .json(&json!({
            "secret_shares": 1,
            "secret_threshold": 1
        }))
        .send()
        .await
        .context("initialize Vault")?;
    if !response.status().is_success() {
        return Err(SecretError::VaultInitFailed(response.status()).into());
    }
    let payload: VaultInitResponse = response
        .json()
        .await
        .context("decode Vault init response")?;
    Ok(VaultBootstrapMaterial {
        root_token: payload.root_token,
        unseal_keys_b64: if payload.unseal_keys_b64.is_empty() {
            payload.keys_base64
        } else {
            payload.unseal_keys_b64
        },
        initialized_at: Utc::now().to_rfc3339(),
    })
}

async fn unseal_vault(client: &Client, addr: &str, key: &str) -> Result<()> {
    let url = format!("{}/v1/sys/unseal", addr.trim_end_matches('/'));
    let response = client
        .put(url)
        .json(&json!({ "key": key }))
        .send()
        .await
        .context("unseal Vault")?;
    if !response.status().is_success() {
        return Err(SecretError::VaultUnsealFailed(response.status()).into());
    }
    Ok(())
}

async fn ensure_kv_v2_mount(
    client: &Client,
    addr: &str,
    root_token: &str,
    mount: &str,
) -> Result<()> {
    let mount_name = mount.trim_matches('/');
    let mounts_url = format!("{}/v1/sys/mounts", addr.trim_end_matches('/'));
    let mounts: serde_json::Value = client
        .get(&mounts_url)
        .header("X-Vault-Token", root_token)
        .send()
        .await
        .context("query Vault mounts")?
        .error_for_status()
        .context("query Vault mounts status")?
        .json()
        .await
        .context("decode Vault mounts")?;

    let key = format!("{mount_name}/");
    if let Some(existing) = mounts.get(&key) {
        let mount_type = existing
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let version = existing
            .get("options")
            .and_then(|value| value.get("version"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if mount_type == "kv" && version == "2" {
            return Ok(());
        }
        return Err(SecretError::VaultMountNotKvV2(mount_name.to_string()).into());
    }

    let enable_url = format!(
        "{}/v1/sys/mounts/{}",
        addr.trim_end_matches('/'),
        mount_name
    );
    let response = client
        .post(enable_url)
        .header("X-Vault-Token", root_token)
        .json(&json!({
            "type": "kv",
            "options": { "version": "2" }
        }))
        .send()
        .await
        .context("enable Vault kv-v2 mount")?;
    if !response.status().is_success() {
        return Err(SecretError::VaultMountCreationFailed(response.status()).into());
    }
    Ok(())
}

async fn write_policy(client: &Client, env: &VaultEnv, root_token: &str) -> Result<()> {
    let policy = format!(
        r#"
path "{mount}/data/{prefix}/*" {{
  capabilities = ["create", "read", "update", "delete", "list"]
}}

path "{mount}/metadata/{prefix}/*" {{
  capabilities = ["read", "list"]
}}

path "sys/health" {{
  capabilities = ["read"]
}}
"#,
        mount = env.mount.trim_matches('/'),
        prefix = env.prefix.trim_matches('/')
    );
    let url = format!(
        "{}/v1/sys/policies/acl/{}",
        env.addr.trim_end_matches('/'),
        OPS_POLICY_NAME
    );
    let response = client
        .put(url)
        .header("X-Vault-Token", root_token)
        .json(&json!({ "policy": policy }))
        .send()
        .await
        .context("write Vault policy")?;
    if !response.status().is_success() {
        return Err(SecretError::VaultPolicyFailed(response.status()).into());
    }
    Ok(())
}

async fn create_ops_token(client: &Client, env: &VaultEnv, root_token: &str) -> Result<String> {
    let url = format!(
        "{}/v1/auth/token/create-orphan",
        env.addr.trim_end_matches('/')
    );
    let response = client
        .post(url)
        .header("X-Vault-Token", root_token)
        .json(&json!({
            "display_name": OPS_DISPLAY_NAME,
            "policies": [OPS_POLICY_NAME],
            "renewable": true
        }))
        .send()
        .await
        .context("create Vault ops token")?;
    if !response.status().is_success() {
        return Err(SecretError::VaultTokenCreationFailed(response.status()).into());
    }
    let payload: VaultTokenCreateResponse = response
        .json()
        .await
        .context("decode Vault token create response")?;
    Ok(payload.auth.client_token)
}

async fn token_is_usable(client: &Client, env: &VaultEnv) -> Result<bool> {
    let url = format!(
        "{}/v1/auth/token/lookup-self",
        env.addr.trim_end_matches('/')
    );
    let response = client
        .get(url)
        .header("X-Vault-Token", &env.token)
        .send()
        .await
        .context("lookup Vault token")?;
    Ok(response.status().is_success())
}

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
        let bootstrap = bootstrap
            .as_ref()
            .ok_or_else(|| anyhow!("Vault is sealed but bootstrap material is missing"))?;
        let key = bootstrap
            .unseal_keys_b64
            .first()
            .ok_or_else(|| anyhow!("Vault bootstrap material is missing unseal keys"))?;
        unseal_vault(&client, &addr, key).await?;
    }

    let bootstrap = load_bootstrap_material()?
        .ok_or_else(|| anyhow!("Vault bootstrap material missing after initialization"))?;
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

pub async fn run_secrets_init(db: Option<&Db>) -> Result<VaultStatusReport> {
    let env = build_operational_env().await?;
    let client = Client::builder().build().context("build HTTP client")?;
    let health = fetch_vault_health(&client, &env.addr).await?;
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
    Ok(VaultStatusReport {
        addr: env.addr,
        initialized: health.initialized,
        sealed: health.sealed,
        healthy: health.initialized && !health.sealed,
        token_present: !env.token.is_empty(),
        mount: env.mount,
        prefix: env.prefix,
        bootstrap_file: config::vault_bootstrap_file().display().to_string(),
        env_file: config::vault_env_file().display().to_string(),
    })
}

pub async fn vault_status(db: Option<&Db>) -> Result<VaultStatusReport> {
    let env = build_operational_env().await?;
    let client = Client::builder().build().context("build HTTP client")?;
    let health = fetch_vault_health(&client, &env.addr).await?;
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
    Ok(VaultStatusReport {
        addr: env.addr,
        initialized: health.initialized,
        sealed: health.sealed,
        healthy: health.initialized && !health.sealed,
        token_present: !env.token.is_empty(),
        mount: env.mount,
        prefix: env.prefix,
        bootstrap_file: config::vault_bootstrap_file().display().to_string(),
        env_file: config::vault_env_file().display().to_string(),
    })
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

async fn run_in_repo(repo_root: &Path, args: &[String], label: &str) -> Result<()> {
    let status = Command::new("cargo")
        .current_dir(repo_root)
        .args(args)
        .status()
        .await
        .with_context(|| format!("run {label}"))?;
    if !status.success() {
        return Err(SecretError::CommandFailed(label.to_string(), status.code()).into());
    }
    Ok(())
}

fn audit_path(repo_root: &Path, version: &str) -> PathBuf {
    repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/secret-rotation-audit.json")
}

fn rendered_runtime_path(repo_root: &Path, version: &str) -> PathBuf {
    repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/prod.runtime.env")
}

fn release_report_path(repo_root: &Path, version: &str) -> PathBuf {
    repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/release-handoff.pdf")
}

fn bundle_path(repo_root: &Path, version: &str) -> PathBuf {
    repo_root
        .join("ops/releases")
        .join(version)
        .join("rendered/release-secrets.enc")
}

fn parse_audit_paths(path: &Path) -> Result<(Option<String>, Option<String>, Option<String>)> {
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
    let args = vec![
        "run".to_string(),
        "-p".to_string(),
        "veox-deploy".to_string(),
        "--".to_string(),
        "rotate-runtime-secrets".to_string(),
        "--version".to_string(),
        version.to_string(),
        "--deploy-env".to_string(),
        rendered_deploy_env.display().to_string(),
        "--runtime-env".to_string(),
        runtime_env_path.display().to_string(),
        "--target".to_string(),
        target.as_str().to_string(),
    ];
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
            runtime_secret_vault_path.clone().unwrap_or_default()
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
    let args = vec![
        "run".to_string(),
        "-p".to_string(),
        "veox-deploy".to_string(),
        "--".to_string(),
        "finalize-secret-rotation".to_string(),
        "--version".to_string(),
        version.to_string(),
        "--deploy-env".to_string(),
        rendered_deploy_env.display().to_string(),
        "--runtime-env".to_string(),
        runtime_env_path.display().to_string(),
        "--target".to_string(),
        target.as_str().to_string(),
    ];
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
    let args = vec![
        "run".to_string(),
        "-p".to_string(),
        "veox-deploy".to_string(),
        "--".to_string(),
        "build-release-handoff-report".to_string(),
        "--version".to_string(),
        version.to_string(),
    ];
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
    let args = vec![
        "run".to_string(),
        "-p".to_string(),
        "veox-deploy".to_string(),
        "--".to_string(),
        "print-release-recovery-instructions".to_string(),
        "--version".to_string(),
        version.to_string(),
    ];
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
    let root = repo_root();
    (
        root.clone(),
        root.join("env/prod.deploy.env"),
        root.join("env/prod.runtime.env"),
    )
}
