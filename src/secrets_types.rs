use anyhow::Result;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Typed errors for Vault secrets lifecycle.
#[derive(Debug, Error)]
pub enum SecretError {
    #[error("unknown secret target: {0}")]
    UnknownTarget(String),
    #[error("Vault did not become reachable at {0}")]
    VaultUnreachable(String),
    #[error("unexpected Vault health status: {0}")]
    VaultUnexpectedStatus(StatusCode),
    #[error("Vault init failed: {0}")]
    VaultInitFailed(StatusCode),
    #[error("Vault unseal failed: {0}")]
    VaultUnsealFailed(StatusCode),
    #[error("Vault mount `{0}` exists but is not kv-v2")]
    VaultMountNotKvV2(String),
    #[error("Vault mount creation failed: {0}")]
    VaultMountCreationFailed(StatusCode),
    #[error("writing Vault policy failed: {0}")]
    VaultPolicyFailed(StatusCode),
    #[error("creating Vault ops token failed: {0}")]
    VaultTokenCreationFailed(StatusCode),
    #[error("{0} failed with exit code {1:?}")]
    CommandFailed(String, Option<i32>),
}

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
pub struct VaultBootstrapMaterial {
    pub root_token: String,
    pub unseal_keys_b64: Vec<String>,
    pub initialized_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEnv {
    pub addr: String,
    pub token: String,
    pub mount: String,
    pub prefix: String,
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

impl VaultStatusReport {
    pub fn is_reachable(&self) -> bool {
        self.healthy || self.initialized || self.sealed
    }
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

#[derive(Debug, Deserialize, Default)]
pub struct VaultHealthResponse {
    pub initialized: bool,
    pub sealed: bool,
}

#[derive(Debug, Deserialize)]
pub struct VaultInitResponse {
    pub root_token: String,
    #[serde(default)]
    pub unseal_keys_b64: Vec<String>,
    #[serde(default)]
    pub keys_base64: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct VaultTokenCreateResponse {
    pub auth: VaultAuth,
}

#[derive(Debug, Deserialize)]
pub struct VaultAuth {
    pub client_token: String,
}
