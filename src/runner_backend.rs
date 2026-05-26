//! Owner: Runner Backend Abstraction
//! Proof: `cargo test -p jeryu -- runner_backend`
//! Invariants: All backends produce Manager rows satisfying the state machine
//!             (starting → online → draining → stopped | node_unreachable).
//!             Local Docker backend preserves existing behaviour exactly.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeSet;

use crate::state::Pool;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Opaque handle returned by `start_manager`. The backend_id is stored in
/// the `docker_container_id` DB column (kept for backward compatibility;
/// semantically it now holds a container ID, pod name, or other backend ref).
#[derive(Debug, Clone)]
pub struct ManagerHandle {
    /// Container ID (local/remote Docker) or pod name (K8s).
    pub backend_id: String,
    /// Config directory path (Docker) or ConfigMap name (K8s).
    pub config_ref: String,
}

/// A running manager instance as seen by the backend.
#[derive(Debug, Clone)]
pub struct RunningManager {
    pub backend_id: String,
    pub pool_name: String,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Pluggable backend for runner-manager lifecycle operations.
///
/// Three implementations exist:
/// - `LocalDockerBackend`  — uses the local Docker daemon (existing behaviour)
/// - `RemoteDockerBackend` — SSH-orchestrates Docker on a remote node
/// - `K8sBackend`          — manages a Kubernetes Deployment (optional feature)
#[async_trait]
pub trait RunnerBackend: Send + Sync {
    /// Human-readable label used in logs and TUI.
    fn backend_label(&self) -> &str;

    /// Start a single runner-manager instance.
    /// Returns a `ManagerHandle` whose `backend_id` is stored in the DB.
    async fn start_manager(
        &self,
        pool_name: &str,
        manager_id: &str,
        pool: &Pool,
        gitlab_url: &str,
    ) -> Result<ManagerHandle>;

    /// Stop (drain + remove) one manager instance.
    async fn stop_manager(&self, backend_id: &str, drain_timeout_secs: i64) -> Result<()>;

    /// Return the backend IDs of all running managed containers / pods.
    /// Used by the reconciler to detect crashed managers.
    async fn list_running_backend_ids(&self) -> Result<BTreeSet<String>>;

    /// Fetch recent logs for a manager.
    async fn get_manager_logs(&self, backend_id: &str, lines: usize) -> Result<String>;

    /// Reload config (SIGHUP) for a running manager after token rotation.
    /// Default no-op; backends that support in-place reload override this.
    async fn reload_manager_config(&self, _backend_id: &str) -> Result<()> {
        Ok(())
    }

    /// For K8s Deployment-based backends: directly patch replica count.
    /// Default no-op for Docker-based backends.
    async fn set_desired_replicas(&self, _pool_name: &str, _count: usize) -> Result<()> {
        Ok(())
    }

    /// Storage garbage collection. Called after every reconciliation cycle.
    /// Default no-op for K8s (PVC lifecycle managed by K8s).
    async fn gc_storage(&self, _max_gib: f64) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MockBackend — used in unit tests
// ---------------------------------------------------------------------------

/// In-memory backend for unit tests. No Docker or SSH required.
#[cfg(test)]
#[allow(unused_imports)]
use std::sync::{Arc, Mutex};

#[cfg(test)]
#[derive(Debug, Default, Clone)]
pub struct MockBackend {
    pub running: Arc<Mutex<BTreeSet<String>>>,
    pub started_count: Arc<Mutex<usize>>,
    pub stopped_ids: Arc<Mutex<Vec<String>>>,
    /// If set, `start_manager` returns this error.
    pub fail_start: Arc<Mutex<bool>>,
}

#[cfg(test)]
#[async_trait]
impl RunnerBackend for MockBackend {
    fn backend_label(&self) -> &str {
        "mock"
    }

    async fn start_manager(
        &self,
        _pool_name: &str,
        manager_id: &str,
        _pool: &Pool,
        _gitlab_url: &str,
    ) -> Result<ManagerHandle> {
        if *self.fail_start.lock().unwrap() {
            anyhow::bail!("MockBackend: start_manager failure injected");
        }
        let backend_id = format!("mock-container-{}", manager_id);
        self.running.lock().unwrap().insert(backend_id.clone());
        *self.started_count.lock().unwrap() += 1;
        Ok(ManagerHandle {
            backend_id,
            config_ref: format!("mock-config-{}", manager_id),
        })
    }

    async fn stop_manager(&self, backend_id: &str, _drain_timeout_secs: i64) -> Result<()> {
        self.running.lock().unwrap().remove(backend_id);
        self.stopped_ids
            .lock()
            .unwrap()
            .push(backend_id.to_string());
        Ok(())
    }

    async fn list_running_backend_ids(&self) -> Result<BTreeSet<String>> {
        Ok(self.running.lock().unwrap().clone())
    }

    async fn get_manager_logs(&self, backend_id: &str, _lines: usize) -> Result<String> {
        Ok(format!("mock logs for {}", backend_id))
    }
}
