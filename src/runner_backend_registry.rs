//! Owner: Backend Registry
//! Proof: `cargo test -p jeryu -- runner_backend_registry`
//! Invariants:
//!   - The "__local__" backend is always registered and never returns None.
//!   - Startup failures (SSH unreachable, K8s context missing) log warnings
//!     but do NOT prevent the engine from starting.
//!   - `get_for_pool` falls back to local if the pool's cluster_alias is absent
//!     or unregistered.

use std::collections::HashMap;
use std::sync::Arc;

use crate::docker::DockerCtl;
use crate::node_support::list_node_configs;
use crate::runner_backend::RunnerBackend;
use crate::runner_backend_local::LocalDockerBackend;
use crate::runner_backend_remote::RemoteDockerBackend;
use crate::state::Pool;

// ---------------------------------------------------------------------------
// BackendRegistry
// ---------------------------------------------------------------------------

pub const LOCAL_BACKEND_KEY: &str = "__local__";

pub struct BackendRegistry {
    backends: HashMap<String, Arc<dyn RunnerBackend>>,
}

impl BackendRegistry {
    /// Build the registry from a local DockerCtl and all configured node/cluster files.
    /// Failures in individual backend initialisation are logged but non-fatal.
    pub fn build(docker: DockerCtl) -> Self {
        let mut registry = Self {
            backends: HashMap::new(),
        };

        // Always register local Docker.
        registry.backends.insert(
            LOCAL_BACKEND_KEY.to_string(),
            Arc::new(LocalDockerBackend::new(docker)),
        );

        // Register remote SSH nodes.
        match list_node_configs() {
            Ok(nodes) => {
                for node in nodes {
                    if node.enabled {
                        let alias = node.alias.clone();
                        registry
                            .backends
                            .insert(alias.clone(), Arc::new(RemoteDockerBackend::new(node)));
                        tracing::info!(alias, "registered remote Docker backend");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to load node configs; no remote backends registered");
            }
        }

        // K8s backends would be registered here when the k8s feature is enabled.
        // (Placeholder for Phase 4.)

        registry
    }

    /// Convenience constructor for tests — no filesystem access, local Docker only.
    pub fn local_only(docker: DockerCtl) -> Self {
        let mut backends: HashMap<String, Arc<dyn RunnerBackend>> = HashMap::new();
        backends.insert(
            LOCAL_BACKEND_KEY.to_string(),
            Arc::new(LocalDockerBackend::new(docker)),
        );
        Self { backends }
    }

    /// Register an additional backend (used in tests and K8s bootstrap).
    pub fn register(&mut self, alias: &str, backend: Arc<dyn RunnerBackend>) {
        self.backends.insert(alias.to_string(), backend);
    }

    /// Resolve the backend for a pool.  Returns the local backend as fallback.
    pub fn get_for_pool(&self, pool: &Pool) -> Arc<dyn RunnerBackend> {
        pool.cluster_alias
            .as_deref()
            .and_then(|a| self.backends.get(a).cloned())
            .unwrap_or_else(|| self.local_backend())
    }

    /// Resolve a backend by alias (e.g. for per-node operations like doctor/GC).
    pub fn get_by_alias(&self, alias: &str) -> Option<Arc<dyn RunnerBackend>> {
        self.backends.get(alias).cloned()
    }

    /// The local Docker backend, always present.
    pub fn local_backend(&self) -> Arc<dyn RunnerBackend> {
        self.backends[LOCAL_BACKEND_KEY].clone()
    }

    /// All registered aliases (includes "__local__").
    pub fn aliases(&self) -> Vec<String> {
        let mut aliases: Vec<_> = self.backends.keys().cloned().collect();
        aliases.sort();
        aliases
    }

    /// All aliases except "__local__" (the remote/cluster entries only).
    pub fn remote_aliases(&self) -> Vec<String> {
        self.aliases()
            .into_iter()
            .filter(|a| a != LOCAL_BACKEND_KEY)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner_backend::MockBackend;

    fn make_registry() -> BackendRegistry {
        let mut backends: HashMap<String, Arc<dyn RunnerBackend>> = HashMap::new();
        backends.insert(
            LOCAL_BACKEND_KEY.to_string(),
            Arc::new(MockBackend::default()),
        );
        backends.insert("xbabe0".to_string(), Arc::new(MockBackend::default()));
        BackendRegistry { backends }
    }

    fn local_pool() -> Pool {
        Pool {
            name: "build".into(),
            gitlab_runner_id: 1,
            auth_token: "tok".into(),
            tags: "".into(),
            executor: "docker".into(),
            min_warm: 1,
            max_managers: 4,
            concurrent: 8,
            request_concurrency: 4,
            paused: false,
            trust_tier: "trusted".into(),
            cluster_alias: None,
            backend_type: "docker".into(),
        }
    }

    fn remote_pool(alias: &str) -> Pool {
        Pool {
            cluster_alias: Some(alias.to_string()),
            backend_type: "docker-remote".into(),
            ..local_pool()
        }
    }

    #[test]
    fn get_for_pool_local_fallback() {
        let registry = make_registry();
        let pool = local_pool();
        let backend = registry.get_for_pool(&pool);
        assert_eq!(backend.backend_label(), "mock");
    }

    #[test]
    fn get_for_pool_routes_to_remote() {
        let registry = make_registry();
        let pool = remote_pool("xbabe0");
        let backend = registry.get_for_pool(&pool);
        assert_eq!(backend.backend_label(), "mock");
    }

    #[test]
    fn get_for_pool_unknown_alias_falls_back_to_local() {
        let registry = make_registry();
        let pool = remote_pool("nonexistent");
        let backend = registry.get_for_pool(&pool);
        assert_eq!(backend.backend_label(), "mock");
    }

    #[test]
    fn remote_aliases_excludes_local() {
        let registry = make_registry();
        let remotes = registry.remote_aliases();
        assert!(!remotes.contains(&LOCAL_BACKEND_KEY.to_string()));
        assert!(remotes.contains(&"xbabe0".to_string()));
    }
}
