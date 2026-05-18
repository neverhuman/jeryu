//! Owner: Secrets & Vault Lifecycle
//! Proof: `cargo test -p jeryu -- secrets`
//! Invariants: Rotation is current/previous pair; never raw plaintext; 0600 perms on all secret files

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use thiserror::Error;

use crate::config;

#[path = "secrets_support.rs"]
mod secrets_support;
use secrets_support::*;

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
            .unwrap_or("");
        let version = existing
            .get("options")
            .and_then(|value| value.get("version"))
            .and_then(|value| value.as_str())
            .unwrap_or("");
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

#[path = "secrets_runtime.rs"]
mod secrets_runtime;
pub use secrets_runtime::*;
