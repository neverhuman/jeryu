//! Owner: Runner Backend — Local Docker
//! Proof: `cargo test -p jeryu -- runner_backend_local`
//! Invariants: Wraps the existing DockerCtl; all behaviour is identical to the
//!             pre-backend-abstraction code.  No Docker calls are changed.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeSet;

use crate::config;
use crate::docker::DockerCtl;
use crate::runner_backend::{ManagerHandle, RunnerBackend};
use crate::state::Pool;

// ---------------------------------------------------------------------------
// LocalDockerBackend
// ---------------------------------------------------------------------------

/// Runner backend backed by the local Docker daemon.
/// This is a thin wrapper around `DockerCtl` and preserves all existing
/// container-management behaviour exactly.
pub struct LocalDockerBackend {
    docker: DockerCtl,
}

impl LocalDockerBackend {
    pub fn new(docker: DockerCtl) -> Self {
        Self { docker }
    }

    /// Convenience: borrow the inner DockerCtl for operations that aren't
    /// yet abstracted (compose, volume pruning, etc.).
    pub fn docker(&self) -> &DockerCtl {
        &self.docker
    }
}

#[async_trait]
impl RunnerBackend for LocalDockerBackend {
    fn backend_label(&self) -> &str {
        "docker-local"
    }

    async fn start_manager(
        &self,
        pool_name: &str,
        manager_id: &str,
        pool: &Pool,
        gitlab_url: &str,
    ) -> Result<ManagerHandle> {
        use std::fs;

        let config_dir = config::runners_dir()
            .join(manager_id)
            .display()
            .to_string();
        let manager_cache_dir = config::manager_cache_dir(manager_id);
        let pool_cache_dir = config::pool_cache_root(pool_name);
        let pool_targets_dir = config::pool_cargo_targets_root(pool_name);
        let pool_sccache_dir = config::pool_cargo_sccache_dir(pool_name);

        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&manager_cache_dir)?;
        fs::create_dir_all(&pool_targets_dir)?;
        fs::create_dir_all(&pool_sccache_dir)?;

        let config_content = config::render_runner_config(
            pool_name,
            manager_id,
            gitlab_url,
            &pool.auth_token,
            &pool.executor,
            &pool_cache_dir.display().to_string(),
            pool.concurrent,
            pool.request_concurrency,
        );
        fs::write(format!("{config_dir}/config.toml"), &config_content)?;

        let container_id = self
            .docker
            .start_runner_manager(
                manager_id,
                &config_dir,
                &manager_cache_dir.display().to_string(),
                &pool_cache_dir.display().to_string(),
                &pool.executor,
                None,
            )
            .await?;

        Ok(ManagerHandle {
            backend_id: container_id,
            config_ref: config_dir,
        })
    }

    async fn stop_manager(&self, backend_id: &str, drain_timeout_secs: i64) -> Result<()> {
        self.docker.cleanup_runner_cache(backend_id).await.ok();
        self.docker
            .drain_runner_manager(backend_id, drain_timeout_secs)
            .await
            .ok();
        self.docker.cleanup_runner_cache(backend_id).await.ok();
        self.docker.remove_runner_manager(backend_id).await.ok();
        Ok(())
    }

    async fn list_running_backend_ids(&self) -> Result<BTreeSet<String>> {
        self.docker.running_managed_container_ids().await
    }

    async fn get_manager_logs(&self, backend_id: &str, lines: usize) -> Result<String> {
        let entries = self.docker.manager_logs(backend_id, lines).await?;
        Ok(entries.join("\n"))
    }

    async fn reload_manager_config(&self, backend_id: &str) -> Result<()> {
        self.docker.reload_runner_config(backend_id).await.ok();
        Ok(())
    }
}
