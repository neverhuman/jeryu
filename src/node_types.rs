//! Owner: Node Configuration (SSH remote Docker nodes)
//! Proof: `cargo test -p jeryu -- node_types`
//! Invariants: NodeConfig round-trips through TOML without loss.
//!             ssh_port default is 22; storage_limit_gb default is 100.0.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::remote::{RemoteConfig, build_remote_connection};

const DEFAULT_RUNNER_DATA_DIR: &str = "~/.jeryu/runners";
const DEFAULT_RUNNER_CACHE_DIR: &str = "~/.jeryu/cache";
const DEFAULT_DOCKER_SOCKET: &str = "/var/run/docker.sock";
const DEFAULT_MAX_MANAGERS: usize = 4;
pub(crate) const DEFAULT_STORAGE_LIMIT_GB: f64 = 100.0;

/// Configuration for a remote SSH node that hosts runner-manager containers.
/// Stored at `~/.jeryu/nodes/<alias>.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub alias: String,
    /// SSH target, e.g. `"deploy@xbabe0"` or `"ubuntu@10.0.1.5"`.
    pub target: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    /// Path to the SSH identity file.  `None` = use ssh-agent / default key.
    pub identity: Option<String>,

    // --- Docker ---
    #[serde(default = "default_docker_socket")]
    pub docker_socket: String,
    /// Remote base directory for per-manager runner configs.
    #[serde(default = "default_runner_data_dir")]
    pub runner_data_dir: String,
    /// Remote base directory for cache volumes.
    #[serde(default = "default_runner_cache_dir")]
    pub runner_cache_dir: String,

    // --- Capacity ---
    #[serde(default = "default_max_managers")]
    pub max_managers: usize,
    /// Total storage budget (GiB) on this node.  Auto-GC triggers at 90 %.
    #[serde(default = "default_storage_limit_gb")]
    pub storage_limit_gb: f64,

    // --- Routing ---
    /// Pool names this node accepts.  Empty = accept all pools.
    #[serde(default)]
    pub pool_affinity: Vec<String>,
    /// Override the GitLab URL passed to runners on this node.
    /// Use when the control-plane GitLab is not reachable via the default
    /// `http://gitlab.local:<port>` address from the remote node.
    pub gitlab_url_override: Option<String>,

    // --- Metadata ---
    /// Set to `false` to put the node into maintenance mode.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at_utc: String,
}

fn default_ssh_port() -> u16 {
    22
}
fn default_docker_socket() -> String {
    DEFAULT_DOCKER_SOCKET.to_string()
}
fn default_runner_data_dir() -> String {
    DEFAULT_RUNNER_DATA_DIR.to_string()
}
fn default_runner_cache_dir() -> String {
    DEFAULT_RUNNER_CACHE_DIR.to_string()
}
fn default_max_managers() -> usize {
    DEFAULT_MAX_MANAGERS
}
fn default_storage_limit_gb() -> f64 {
    DEFAULT_STORAGE_LIMIT_GB
}
fn default_enabled() -> bool {
    true
}

impl NodeConfig {
    /// Build a minimal `NodeConfig` from the required fields.
    pub fn new(alias: String, target: String) -> Self {
        Self {
            alias,
            target,
            ssh_port: 22,
            identity: None,
            docker_socket: DEFAULT_DOCKER_SOCKET.to_string(),
            runner_data_dir: DEFAULT_RUNNER_DATA_DIR.to_string(),
            runner_cache_dir: DEFAULT_RUNNER_CACHE_DIR.to_string(),
            max_managers: DEFAULT_MAX_MANAGERS,
            storage_limit_gb: DEFAULT_STORAGE_LIMIT_GB,
            pool_affinity: vec![],
            gitlab_url_override: None,
            enabled: true,
            created_at_utc: Utc::now().to_rfc3339(),
        }
    }

    /// Derive an alias from the SSH target string (mirrors `default_alias`).
    pub fn derive_alias(target: &str) -> String {
        target
            .rsplit('@')
            .next()
            .unwrap_or(target)
            .replace([':', '/'], "-")
    }

    /// Build an ephemeral `RemoteConfig` so this node can reuse all existing
    /// SSH helper functions (`ssh_args`, `ssh_bash_command`, etc.) from
    /// `remote_support.rs` without duplication.
    pub fn as_remote_config(&self) -> RemoteConfig {
        let conn = build_remote_connection(
            self.alias.clone(),
            self.target.clone(),
            self.ssh_port,
            self.identity.clone(),
        );
        RemoteConfig {
            connection: conn,
            created_at_utc: self.created_at_utc.clone(),
            service_mode: crate::remote::ServiceMode::Manual,
        }
    }

    /// Returns true if this node accepts jobs for the given pool name.
    /// An empty `pool_affinity` list means "accept all pools".
    pub fn accepts_pool(&self, pool_name: &str) -> bool {
        self.pool_affinity.is_empty()
            || self.pool_affinity.iter().any(|p| p == pool_name)
    }
}

// ---------------------------------------------------------------------------
// Storage report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NodeStorageReport {
    pub alias: String,
    /// Total storage used under `runner_cache_dir` in GiB.
    pub used_gb: f64,
    /// Configured limit in GiB.
    pub limit_gb: f64,
}

impl NodeStorageReport {
    pub fn is_over_threshold(&self) -> bool {
        self.used_gb >= self.limit_gb * 0.9
    }

    pub fn is_critical(&self) -> bool {
        self.used_gb >= self.limit_gb * 0.95
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_config_toml_round_trip() {
        let cfg = NodeConfig::new("xbabe0".to_string(), "deploy@xbabe0".to_string());
        let toml = toml::to_string_pretty(&cfg).expect("serialize");
        let decoded: NodeConfig = toml::from_str(&toml).expect("deserialize");
        assert_eq!(decoded.alias, cfg.alias);
        assert_eq!(decoded.target, cfg.target);
        assert_eq!(decoded.max_managers, DEFAULT_MAX_MANAGERS);
        assert!((decoded.storage_limit_gb - DEFAULT_STORAGE_LIMIT_GB).abs() < 0.01);
        assert!(decoded.enabled);
        assert!(decoded.pool_affinity.is_empty());
    }

    #[test]
    fn node_config_accepts_pool_empty_affinity() {
        let cfg = NodeConfig::new("n0".to_string(), "u@h".to_string());
        assert!(cfg.accepts_pool("build"));
        assert!(cfg.accepts_pool("test"));
    }

    #[test]
    fn node_config_accepts_pool_with_affinity() {
        let mut cfg = NodeConfig::new("n0".to_string(), "u@h".to_string());
        cfg.pool_affinity = vec!["build".to_string()];
        assert!(cfg.accepts_pool("build"));
        assert!(!cfg.accepts_pool("test"));
    }

    #[test]
    fn derive_alias_strips_user() {
        assert_eq!(NodeConfig::derive_alias("deploy@xbabe0"), "xbabe0");
        assert_eq!(NodeConfig::derive_alias("root@10.0.1.5"), "10.0.1.5");
        assert_eq!(NodeConfig::derive_alias("xbabe1"), "xbabe1");
    }

    #[test]
    fn as_remote_config_preserves_ssh_fields() {
        let mut cfg = NodeConfig::new("xbabe0".to_string(), "deploy@xbabe0".to_string());
        cfg.ssh_port = 2222;
        cfg.identity = Some("/home/user/.ssh/id_ed25519".to_string());

        let rc = cfg.as_remote_config();
        assert_eq!(rc.alias, "xbabe0");
        assert_eq!(rc.target, "deploy@xbabe0");
        assert_eq!(rc.ssh_port, 2222);
        assert_eq!(rc.identity.as_deref(), Some("/home/user/.ssh/id_ed25519"));
    }
}
