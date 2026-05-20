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

pub const DEFAULT_POOLS: &[PoolDef] = &[
    PoolDef {
        name: "default",
        tags: "default,rust,test",
        executor: "docker",
        min_warm: 2,
        max_managers: 4,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "trusted",
    },
    PoolDef {
        name: "build",
        tags: "build,docker-build,x86-64,docker,dind",
        executor: "docker",
        min_warm: 2,
        max_managers: 4,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "privileged",
    },
    PoolDef {
        name: "untrusted",
        tags: "untrusted,sandbox,mr",
        executor: "custom",
        min_warm: 1,
        max_managers: 2,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "untrusted",
    },
];

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProofConfig {
    #[serde(default)]
    pub lanes: std::collections::HashMap<String, Vec<String>>,
    #[serde(default)]
    pub vti: VtiConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VtiConfig {
    #[serde(default)]
    pub ast_aware_skipping: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AgentConfig {
    #[serde(default)]
    pub autonomy: AutonomyConfig,
    #[serde(default)]
    pub context: ContextConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AutonomyConfig {
    #[serde(default)]
    pub auto_merge_remediations: bool,
    #[serde(default)]
    pub budget_limit_usd: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ContextConfig {
    #[serde(default)]
    pub mandatory_context: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SandboxConfig {
    #[serde(default)]
    pub isolation: IsolationConfig,
    #[serde(default)]
    pub exceptions: ExceptionsConfig,
    #[serde(default)]
    pub detonation: DetonationConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct IsolationConfig {
    #[serde(default = "default_egress")]
    pub default_network_egress: String,
}

fn default_egress() -> String {
    "block".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ExceptionsConfig {
    #[serde(default)]
    pub allow_egress: Vec<EgressException>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct EgressException {
    #[serde(default)]
    pub lane: String,
    #[serde(default)]
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DetonationConfig {
    #[serde(default)]
    pub tripwires: Vec<String>,
}

/// Helper to load a TOML configuration file if it exists, otherwise returning the Default.
pub fn load_jeryu_workspace_config<T: serde::de::DeserializeOwned + Default>(
    repo_root: &std::path::Path,
    filename: &str,
) -> T {
    load_jeryu_workspace_config_with_mode(repo_root, filename, ConfigLoadMode::Permissive)
}

pub fn load_jeryu_workspace_config_with_mode<T: serde::de::DeserializeOwned + Default>(
    repo_root: &std::path::Path,
    filename: &str,
    mode: ConfigLoadMode,
) -> T {
    let path = repo_root.join(".jeryu").join(filename);
    if let Ok(contents) = std::fs::read_to_string(&path) {
        match toml::from_str(&contents) {
            Ok(value) => value,
            Err(err) => match mode {
                ConfigLoadMode::Permissive => {
                    eprintln!(
                        "Warning: Failed to parse {}, using defaults: {}",
                        path.display(),
                        err
                    );
                    T::default()
                }
                ConfigLoadMode::FailClosed => {
                    panic!("Failed to parse {}: {}", path.display(), err)
                }
            },
        }
    } else {
        T::default()
    }
}

pub fn load_proof_config(repo_root: &std::path::Path) -> ProofConfig {
    load_jeryu_workspace_config(repo_root, "proof.toml")
}

pub fn load_agent_config(repo_root: &std::path::Path) -> AgentConfig {
    load_jeryu_workspace_config(repo_root, "agent.toml")
}

pub fn load_sandbox_config(repo_root: &std::path::Path) -> SandboxConfig {
    load_jeryu_workspace_config(repo_root, "sandbox.toml")
}

#[cfg(test)]
mod config_support_tests;
