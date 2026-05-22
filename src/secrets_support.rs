//! Owner: Secrets & Vault Lifecycle
//! Proof: `cargo test -p jeryu -- secrets`
//! Invariants: Rotation is current/previous pair; never raw plaintext; 0600 perms on all secret files

use anyhow::{Context, Result};
use chrono::Utc;
use rand::Rng;
use reqwest::Client;
use serde_json::json;
use std::fs;
use std::path::Path;

use super::{SecretError, VaultBootstrapMaterial, VaultEnv, config};

#[path = "secrets_support_env.rs"]
mod env;

pub(super) use env::*;

pub(super) fn random_alnum(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..ALPHABET.len());
            ALPHABET[idx] as char
        })
        .collect()
}

pub(super) fn write_restricted(path: &Path, contents: &str) -> Result<()> {
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

pub(super) async fn ensure_vault_files() -> Result<()> {
    fs::create_dir_all(config::vault_storage_dir())
        .with_context(|| format!("create {}", config::vault_storage_dir().display()))?;
    Ok(())
}

pub(super) fn load_bootstrap_material() -> Result<Option<VaultBootstrapMaterial>> {
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

pub(super) fn save_bootstrap_material(material: &VaultBootstrapMaterial) -> Result<()> {
    let payload = serde_json::to_string_pretty(material)?;
    write_restricted(&config::vault_bootstrap_file(), &payload)
}

pub(super) async fn wait_for_vault_http(client: &Client, addr: &str) -> Result<()> {
    let url = format!("{}/v1/sys/health", addr.trim_end_matches('/'));
    for _ in 0..60 {
        match client.get(&url).send().await {
            Ok(_) => return Ok(()),
            Err(_) => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
        }
    }
    Err(SecretError::VaultUnreachable(addr.to_string()).into())
}

pub(super) async fn fetch_vault_health(
    client: &Client,
    addr: &str,
) -> Result<super::VaultHealthResponse> {
    let url = format!("{}/v1/sys/health", addr.trim_end_matches('/'));
    let response = client.get(url).send().await.context("query Vault health")?;
    match response.status() {
        reqwest::StatusCode::OK
        | reqwest::StatusCode::TOO_MANY_REQUESTS
        | reqwest::StatusCode::BAD_REQUEST
        | reqwest::StatusCode::NOT_IMPLEMENTED
        | reqwest::StatusCode::SERVICE_UNAVAILABLE => response
            .json()
            .await
            .context("decode Vault health response"),
        status => Err(SecretError::VaultUnexpectedStatus(status).into()),
    }
}

pub(super) async fn initialize_vault(
    client: &Client,
    addr: &str,
) -> Result<VaultBootstrapMaterial> {
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
    let payload: super::VaultInitResponse = response
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

pub(super) async fn unseal_vault(client: &Client, addr: &str, key: &str) -> Result<()> {
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
