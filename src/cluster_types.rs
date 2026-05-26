//! Owner: Cluster Configuration (Kubernetes runner backends)
//! Proof: `cargo test -p jeryu -- cluster_types`
//! Invariants: ClusterConfig round-trips through TOML without loss.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Configuration for a Kubernetes cluster hosting GitLab runner Deployments.
/// Stored at `~/.jeryu/clusters/<alias>.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub alias: String,
    /// Path to kubeconfig.  `None` = use `KUBECONFIG` env / `~/.kube/config`.
    pub kubeconfig: Option<String>,
    /// Kubernetes context name.  `None` = use current-context.
    pub context: Option<String>,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default = "default_storage_class")]
    pub storage_class: String,
    /// GitLab runner container image.
    pub runner_image: String,
    /// GitLab URL that runners inside the cluster will call.
    pub gitlab_url: String,
    #[serde(default)]
    pub resources: K8sResources,
    #[serde(default)]
    pub runner_mode: K8sRunnerMode,
    #[serde(default)]
    pub node_selector: HashMap<String, String>,
    #[serde(default)]
    pub tolerations: Vec<K8sToleration>,
    pub created_at_utc: String,
}

fn default_namespace() -> String {
    "jeryu".to_string()
}
fn default_storage_class() -> String {
    "standard".to_string()
}

/// Kubernetes resource requests and limits for runner pods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sResources {
    #[serde(default = "default_cpu_request")]
    pub cpu_request: String,
    #[serde(default = "default_cpu_limit")]
    pub cpu_limit: String,
    #[serde(default = "default_mem_request")]
    pub memory_request: String,
    #[serde(default = "default_mem_limit")]
    pub memory_limit: String,
    /// PVC size in GiB (one per pool).
    #[serde(default = "default_cache_gib")]
    pub cache_storage_gib: u32,
}

impl Default for K8sResources {
    fn default() -> Self {
        Self {
            cpu_request: default_cpu_request(),
            cpu_limit: default_cpu_limit(),
            memory_request: default_mem_request(),
            memory_limit: default_mem_limit(),
            cache_storage_gib: default_cache_gib(),
        }
    }
}

fn default_cpu_request() -> String { "500m".to_string() }
fn default_cpu_limit() -> String { "2".to_string() }
fn default_mem_request() -> String { "512Mi".to_string() }
fn default_mem_limit() -> String { "2Gi".to_string() }
fn default_cache_gib() -> u32 { 100 }

/// How the runner pod executes CI jobs.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum K8sRunnerMode {
    /// K8s executor: runner spawns a new pod per CI job (preferred).
    #[default]
    Kubernetes,
    /// Docker-in-Docker sidecar in the runner pod.
    Docker,
}

/// Kubernetes pod toleration spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sToleration {
    pub key: Option<String>,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub effect: Option<String>,
}

impl ClusterConfig {
    pub fn new(alias: String, gitlab_url: String, runner_image: String) -> Self {
        Self {
            alias,
            kubeconfig: None,
            context: None,
            namespace: default_namespace(),
            storage_class: default_storage_class(),
            runner_image,
            gitlab_url,
            resources: K8sResources::default(),
            runner_mode: K8sRunnerMode::default(),
            node_selector: HashMap::new(),
            tolerations: vec![],
            created_at_utc: Utc::now().to_rfc3339(),
        }
    }
}

// ---------------------------------------------------------------------------
// Health snapshot (used by TUI)
// ---------------------------------------------------------------------------

/// Health snapshot for a K8s cluster's runner deployment, used by the TUI.
#[derive(Debug, Clone, Default)]
pub struct ClusterHealthEntry {
    pub alias: String,
    pub namespace: String,
    pub ready_pods: i32,
    pub desired_pods: i32,
    pub reachable: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_config_toml_round_trip() {
        let cfg = ClusterConfig::new(
            "k8s-prod".to_string(),
            "https://gitlab.example.com".to_string(),
            "gitlab/gitlab-runner:v17.9.2".to_string(),
        );
        let toml = toml::to_string_pretty(&cfg).expect("serialize");
        let decoded: ClusterConfig = toml::from_str(&toml).expect("deserialize");
        assert_eq!(decoded.alias, "k8s-prod");
        assert_eq!(decoded.namespace, "jeryu");
        assert_eq!(decoded.resources.cache_storage_gib, 100);
        assert_eq!(decoded.runner_mode, K8sRunnerMode::Kubernetes);
    }

    #[test]
    fn k8s_runner_mode_default_is_kubernetes() {
        let mode: K8sRunnerMode = Default::default();
        assert_eq!(mode, K8sRunnerMode::Kubernetes);
    }
}
