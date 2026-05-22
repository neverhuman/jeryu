use serde::{Deserialize, Serialize};

#[path = "settings_types_tail.rs"]
mod tail;

pub use tail::*;

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
