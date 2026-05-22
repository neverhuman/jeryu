//! Owner: Secrets & Vault Lifecycle
//! Proof: `cargo test -p jeryu -- secrets`
//! Invariants: Rotation is current/previous pair; never raw plaintext; 0600 perms on all secret files

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;

use crate::config;

#[path = "secrets_types.rs"]
mod types;
pub use types::*;

#[path = "secrets_support.rs"]
mod secrets_support;
use secrets_support::*;

const OPS_POLICY_NAME: &str = "jeryu-release-ops";
const OPS_DISPLAY_NAME: &str = "jeryu-release-control-plane";

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
pub use secrets_runtime::vault_status_observed;
pub use secrets_runtime::*;
