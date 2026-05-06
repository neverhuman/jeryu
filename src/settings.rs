//! Owner: User settings subsystem
//! Proof: `cargo nextest run -p jeryu -- settings`
//! Invariants: Settings merges are deterministic and never silently discard explicit user configuration.
//!
//! Loads `~/.jeryu/settings.json`, creating it with defaults on first run.
//! All tunables that were previously env vars live here instead.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub gitlab: GitlabSettings,
    pub vault: VaultSettings,
    pub git: GitSettings,
    pub mirror: MirrorSettings,
    pub webhook: WebhookSettings,
    pub mcp: McpSettings,
    pub pool: PoolSettings,
    pub cache: CacheSettings,
    pub sccache: SccacheSettings,
    pub release: ReleaseSettings,
    pub sandbox: SandboxSettings,
    pub tui: TuiSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitlabSettings {
    /// GitLab container image tag.
    pub image: String,
    /// GitLab Runner image tag.
    pub runner_image: String,
    /// Hostname used in runner config and docker-compose extra_hosts.
    pub hostname: String,
    /// HTTP port exposed by the GitLab container.
    pub http_port: u16,
    /// SSH port exposed by the GitLab container.
    pub ssh_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultSettings {
    /// Vault container image.
    pub image: String,
    /// Docker container name for the Vault instance.
    pub container_name: String,
    /// Port Vault listens on (host-side).
    pub http_port: u16,
    /// KV v2 mount path.
    pub mount: String,
    /// Key prefix within the mount.
    pub prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookSettings {
    /// Bind address for the jeryu webhook/API server.
    /// Defaults to 127.0.0.1 for local-only access.
    pub bind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpSettings {
    /// Bind address for the MCP Streamable HTTP server.
    /// Defaults to 127.0.0.1 for local-only access.
    pub bind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PoolSettings {
    /// Timeout, in seconds, used when waiting for runner managers to exit after SIGQUIT.
    /// Production keeps this high for graceful drains; tests and CI may override it lower.
    pub runner_shutdown_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheSettings {
    /// Port for the crates.io singleflight proxy.
    pub proxy_port: u16,
    /// Port for the OCI registry mirror.
    pub registry_port: u16,
    /// Maximum disk budget for all manager caches (GiB). 0 = unlimited.
    pub manager_budget_gib: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SccacheSettings {
    /// Enable sccache for all CI jobs.
    pub enabled: bool,
    /// Per-manager sccache disk budget. Passed as SCCACHE_CACHE_SIZE.
    pub cache_size: String,
    /// sccache binary version to install in manager containers.
    pub binary_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReleaseSettings {
    /// Path to the release artifact repository root on disk.
    /// Equivalent to the old JERYU_RELEASE_REPO_ROOT env var.
    pub repo_root: Option<String>,
    /// Default GitLab project ID for release tracking.
    pub default_project_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitSettings {
    /// Optional explicit system git binary path.
    pub system_git: Option<String>,
    /// Default git execution mode for the event plane.
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MirrorSettings {
    /// Whether mirror pushes are enabled by default.
    pub enabled: bool,
    /// Preferred mirror remote name.
    pub remote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SandboxSettings {
    /// Enable strict network namespace isolation in the custom executor sandbox.
    /// Equivalent to the old JERYU_STRICT_SANDBOX env var (presence = enabled).
    pub strict_network_isolation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiSettings {
    /// Polling interval for background sync in milliseconds.
    pub sync_interval_ms: u64,
    /// Number of recent jobs to keep in the live jobs list.
    pub recent_jobs_limit: usize,
    /// Number of recent evidence records to display.
    pub recent_evidence_limit: usize,
    /// Number of audit events to keep in memory.
    pub audit_events_limit: usize,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for GitlabSettings {
    fn default() -> Self {
        Self {
            image: "gitlab/gitlab-ce:17.9.2-ce.0".into(),
            runner_image: "gitlab/gitlab-runner:v17.9.2".into(),
            hostname: "gitlab.local".into(),
            http_port: 8929,
            ssh_port: 2224,
        }
    }
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            image: "hashicorp/vault:1.17.5".into(),
            container_name: "jeryu-vault".into(),
            http_port: 18200,
            mount: "secret".into(),
            prefix: "jeryu".into(),
        }
    }
}

impl Default for GitSettings {
    fn default() -> Self {
        Self {
            system_git: None,
            mode: "after_success".into(),
        }
    }
}

impl Default for MirrorSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            remote: "jeryu".into(),
        }
    }
}

impl Default for WebhookSettings {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:9777".into(),
        }
    }
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:9778".into(),
        }
    }
}

impl Default for PoolSettings {
    fn default() -> Self {
        Self {
            runner_shutdown_timeout_secs: 3600,
        }
    }
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            proxy_port: 19800,
            registry_port: 19801,
            manager_budget_gib: 400.0,
        }
    }
}

impl Default for SccacheSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_size: "10G".into(),
            binary_version: "v0.9.1".into(),
        }
    }
}

impl Default for ReleaseSettings {
    fn default() -> Self {
        Self {
            repo_root: Some("/home/ubuntu/dougx".into()),
            default_project_id: 2,
        }
    }
}

impl Default for TuiSettings {
    fn default() -> Self {
        Self {
            sync_interval_ms: 5000,
            recent_jobs_limit: 50,
            recent_evidence_limit: 100,
            audit_events_limit: 50,
        }
    }
}

// ---------------------------------------------------------------------------
// Load / persist
// ---------------------------------------------------------------------------

/// Path to the settings file.
pub fn settings_path() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".jeryu")
        .join("settings.json")
}

/// Load settings from `~/.jeryu/settings.json`.
/// Creates the file with defaults if it does not exist.
/// Unknown keys are ignored (forward-compat); missing keys use defaults (back-compat).
pub fn load() -> Result<Settings> {
    let path = settings_path();

    if !path.exists() {
        let dir = path.parent().expect("settings path has no parent");
        std::fs::create_dir_all(dir)?;
        let defaults = Settings::default();
        let json = serde_json::to_string_pretty(&defaults)?;
        std::fs::write(&path, json)?;
        tracing::info!(path = %path.display(), "created default settings.json");
        return Ok(defaults);
    }

    let raw = std::fs::read_to_string(&path)?;
    let settings: Settings = serde_json::from_str(&raw).map_err(|e| {
        let backup = path.with_file_name(format!(
            "settings.json.bad.{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
        ));
        let _ = std::fs::copy(&path, &backup);
        anyhow::anyhow!(
            "settings.json parse error at {}: {} (backed up to {})",
            path.display(),
            e,
            backup.display()
        )
    })?;
    Ok(settings)
}

// ---------------------------------------------------------------------------
// Process-wide singleton — populated once at startup
// ---------------------------------------------------------------------------

static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// Initialize the process-wide settings. Call once at program entry.
/// Subsequent calls are ignored.
pub fn init() -> Result<()> {
    let s = load()?;
    let _ = SETTINGS.set(s);
    Ok(())
}

/// Access the process-wide settings. Panics if `init()` was not called.
pub fn get() -> &'static Settings {
    SETTINGS.get_or_init(|| load().expect("failed to load settings.json"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let s = Settings::default();
        let json = serde_json::to_string_pretty(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.gitlab.http_port, 8929);
        assert_eq!(s2.git.mode, "after_success");
        assert_eq!(s2.mirror.remote, "jeryu");
        assert_eq!(s2.release.repo_root.as_deref(), Some("/home/ubuntu/dougx"));
        assert_eq!(s2.webhook.bind, "127.0.0.1:9777");
        assert_eq!(s2.mcp.bind, "127.0.0.1:9778");
        assert_eq!(s2.pool.runner_shutdown_timeout_secs, 3600);
        assert!(s2.sccache.enabled);
        assert_eq!(s2.sccache.cache_size, "10G");
        assert_eq!(s2.tui.sync_interval_ms, 5000);
        assert!(!s2.sandbox.strict_network_isolation);
    }

    #[test]
    fn unknown_keys_ignored() {
        let json = r#"{"gitlab": {"http_port": 9999}, "unknown_future_key": true}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.gitlab.http_port, 9999);
        // All other fields should be their defaults
        assert_eq!(s.webhook.bind, "127.0.0.1:9777");
        assert_eq!(s.mcp.bind, "127.0.0.1:9778");
        assert_eq!(s.pool.runner_shutdown_timeout_secs, 3600);
    }

    #[test]
    fn partial_section_uses_defaults() {
        let json = r#"{"sccache": {"cache_size": "20G"}}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.sccache.cache_size, "20G");
        assert!(s.sccache.enabled); // default preserved
        assert_eq!(s.sccache.binary_version, "v0.9.1"); // default preserved
    }
}
