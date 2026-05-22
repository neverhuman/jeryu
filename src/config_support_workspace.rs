use super::ConfigLoadMode;
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
