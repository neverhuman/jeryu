use super::*;

/// Where docker-compose.yml is written.
pub fn compose_file() -> PathBuf {
    data_dir().join("docker-compose.yml")
}

/// Vault persistent data root.
pub fn vault_dir() -> PathBuf {
    data_dir().join("vault")
}

/// Vault runtime configuration directory.
pub fn vault_config_dir() -> PathBuf {
    vault_dir().join("config")
}

/// Vault persistent storage directory.
pub fn vault_storage_dir() -> PathBuf {
    vault_dir().join("data")
}

/// jeryu-managed Vault operational environment file.
pub fn vault_env_file() -> PathBuf {
    vault_dir().join("vault.env")
}

/// Break-glass bootstrap material for Vault.
pub fn vault_bootstrap_file() -> PathBuf {
    vault_dir().join("bootstrap.json")
}

/// Vault server configuration file.
pub fn vault_config_file() -> PathBuf {
    vault_config_dir().join("vault.hcl")
}

/// GitLab persistent volume paths on the host.
pub fn gitlab_config_dir() -> PathBuf {
    data_dir().join("gitlab").join("config")
}
pub fn gitlab_logs_dir() -> PathBuf {
    data_dir().join("gitlab").join("logs")
}
pub fn gitlab_data_dir() -> PathBuf {
    data_dir().join("gitlab").join("data")
}
pub fn gitlab_pre_receive_hooks_dir() -> PathBuf {
    gitlab_data_dir()
        .join("gitaly")
        .join("custom_hooks")
        .join("pre-receive.d")
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

pub const GITLAB_IMAGE: &str = "gitlab/gitlab-ce:17.9.2-ce.0";
pub const GITLAB_RUNNER_IMAGE: &str = "gitlab/gitlab-runner:v17.9.2";
pub const GITLAB_HOSTNAME: &str = "gitlab.local";
pub const LOCAL_DOCKER_NETWORK_NAME: &str = "jeryu_default";
pub const GITLAB_HTTP_PORT: u16 = 8929;
pub const GITLAB_SSH_PORT: u16 = 2224;
pub const WEBHOOK_LISTEN_PORT: u16 = 9777;
pub const VAULT_IMAGE: &str = "hashicorp/vault:1.17.5";
pub const VAULT_CONTAINER_NAME: &str = "jeryu-vault";
pub const VAULT_HTTP_PORT: u16 = 18200;
pub const VAULT_DEFAULT_MOUNT: &str = "secret";
pub const VAULT_DEFAULT_PREFIX: &str = "jeryu";

pub const CACHE_PROXY_PORT: u16 = 19800;
pub const CACHE_REGISTRY_PORT: u16 = 19801;

pub(crate) fn render_vault_local_config() -> String {
    format!(
        r#"ui = true
disable_mlock = true
api_addr = "http://127.0.0.1:{port}"

listener "tcp" {{
  address     = "0.0.0.0:8200"
  tls_disable = 1
}}

storage "file" {{
  path = "/vault/file"
}}
"#,
        port = VAULT_HTTP_PORT
    )
}

pub(crate) fn yaml_block(value: &str, indent: usize) -> String {
    let padding = " ".repeat(indent);
    value
        .lines()
        .map(|line| format!("{padding}{line}\n"))
        .collect::<String>()
}

/// Default pool definitions created during bootstrap.
pub struct PoolDef {
    pub name: &'static str,
    pub tags: &'static str,
    pub executor: &'static str,
    pub min_warm: i64,
    pub max_managers: i64,
    pub concurrent: i64,
    pub request_concurrency: i64,
    pub trust_tier: &'static str,
}

pub const STANDARD_POOL_NAME: &str = "default";
pub const LOCAL_NODE_ALIAS: &str = "__local__";
pub const STANDARD_POOL_DESIRED_TOTAL: i64 = 40;
pub const STANDARD_POOL_REMOTE_NODE_ALIASES: &[&str] = &["xbabe0", "xbabe1", "xbabe3"];
pub const STANDARD_POOL_RESERVED_NODE_ALIASES: &[&str] = &["xbabe2"];

pub struct StandardPoolCapacity {
    pub node_alias: Option<&'static str>,
    pub managers: usize,
}

pub const STANDARD_POOL_TOPOLOGY: &[StandardPoolCapacity] = &[
    StandardPoolCapacity {
        node_alias: None,
        managers: 10,
    },
    StandardPoolCapacity {
        node_alias: Some("xbabe0"),
        managers: 10,
    },
    StandardPoolCapacity {
        node_alias: Some("xbabe1"),
        managers: 10,
    },
    StandardPoolCapacity {
        node_alias: Some("xbabe3"),
        managers: 10,
    },
];

pub const DEFAULT_POOLS: &[PoolDef] = &[
    PoolDef {
        name: STANDARD_POOL_NAME,
        tags: "",
        executor: "docker",
        min_warm: STANDARD_POOL_DESIRED_TOTAL,
        max_managers: STANDARD_POOL_DESIRED_TOTAL,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "trusted",
    },
    PoolDef {
        name: "build",
        tags: "",
        executor: "docker",
        min_warm: 0,
        max_managers: 0,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "privileged",
    },
    PoolDef {
        name: "untrusted",
        tags: "",
        executor: "custom",
        min_warm: 0,
        max_managers: 0,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "untrusted",
    },
];

#[path = "config_support_workspace.rs"]
mod workspace;
pub use workspace::*;
