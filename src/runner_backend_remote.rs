//! Owner: Runner Backend — Remote Docker (SSH)
//! Proof: `cargo test -p jeryu -- runner_backend_remote`
//! Invariants:
//!   - Containers on remote nodes have `--restart unless-stopped` so they
//!     survive SSH drops and control-plane reboots.
//!   - A single SSH connection per reconciliation cycle (one `docker ps` call
//!     covers all managed containers on the node).
//!   - SSH multiplexing (ControlMaster=auto) is inherited from `ssh_args()`.
//!   - GC never removes containers that are still registered in the DB as active.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use std::collections::BTreeSet;
use tracing::{debug, info, warn};

use crate::config;
use crate::node_types::NodeConfig;
use crate::remote::{run_remote_shell, run_remote_shell_capture};
use crate::runner_backend::{ManagerHandle, RunnerBackend};
use crate::runner_backend_remote_support::{
    base64_encode, get_remote_used_kb, runner_bootstrap_cmd_docker, shell_quote,
};
use crate::state::Pool;

// Re-export for integration-test and command use.
pub use crate::runner_backend_remote_support::{
    NodeProbeResult, gc_orphaned_runner_dirs, probe_node,
};

// ---------------------------------------------------------------------------
// RemoteDockerBackend
// ---------------------------------------------------------------------------

pub struct RemoteDockerBackend {
    node: NodeConfig,
}

impl RemoteDockerBackend {
    pub fn new(node: NodeConfig) -> Self {
        Self { node }
    }

    fn remote_config(&self) -> crate::remote::RemoteConfig {
        self.node.as_remote_config()
    }

    fn runner_dir(&self, manager_id: &str) -> String {
        format!("{}/{}", self.node.runner_data_dir, manager_id)
    }

    fn manager_cache_dir(&self, manager_id: &str) -> String {
        format!("{}/managers/{}", self.node.runner_cache_dir, manager_id)
    }

    fn pool_cache_dir(&self, pool_name: &str) -> String {
        format!("{}/pools/{}", self.node.runner_cache_dir, pool_name)
    }

    /// GitLab URL that runners on this node should use.
    fn gitlab_url(&self) -> String {
        if let Some(url) = &self.node.gitlab_url_override {
            return url.clone();
        }
        format!(
            "http://{}:{}",
            config::GITLAB_HOSTNAME,
            config::GITLAB_HTTP_PORT
        )
    }
}

// ---------------------------------------------------------------------------
// RunnerBackend impl
// ---------------------------------------------------------------------------

#[async_trait]
impl RunnerBackend for RemoteDockerBackend {
    fn backend_label(&self) -> &str {
        "docker-remote"
    }

    /// Start a runner-manager container on the remote node via SSH.
    ///
    /// Steps:
    /// 1. Create remote directories.
    /// 2. Write config.toml via base64-encoded heredoc.
    /// 3. `docker run -d` with `--restart unless-stopped`.
    /// 4. Return the container ID from `docker inspect`.
    async fn start_manager(
        &self,
        pool_name: &str,
        manager_id: &str,
        pool: &Pool,
        _gitlab_url: &str,
    ) -> Result<ManagerHandle> {
        let cfg = self.remote_config();
        let runner_dir = self.runner_dir(manager_id);
        let cache_dir = self.manager_cache_dir(manager_id);
        let pool_cache = self.pool_cache_dir(pool_name);
        let gitlab_url = self.gitlab_url();

        // Step 1: create directories.
        let mkdir_script = format!(
            "mkdir -p {runner_dir} {cache_dir} {pool_cache}",
            runner_dir = shell_quote(&runner_dir),
            cache_dir = shell_quote(&cache_dir),
            pool_cache = shell_quote(&pool_cache),
        );
        run_remote_shell(&cfg, &mkdir_script, false)
            .await
            .context("creating remote directories")?;

        // Step 2: write config.toml via base64.
        let config_content = config::render_runner_config(
            pool_name,
            manager_id,
            &gitlab_url,
            &pool.auth_token,
            "docker",
            &pool_cache,
            pool.concurrent,
            pool.request_concurrency,
        );
        let config_b64 = base64_encode(config_content.as_bytes());
        let write_config_script = format!(
            "printf '%s' {b64} | base64 -d > {runner_dir}/config.toml",
            b64 = shell_quote(&config_b64),
            runner_dir = shell_quote(&runner_dir),
        );
        run_remote_shell(&cfg, &write_config_script, false)
            .await
            .context("writing runner config.toml on remote node")?;

        // Step 3: docker run with --restart unless-stopped.
        let container_name = format!("jeryu-runner-{}", manager_id);
        let runner_image = config::GITLAB_RUNNER_IMAGE;
        let bootstrap_cmd = runner_bootstrap_cmd_docker();

        let docker_run = format!(
            "docker run -d \
  --name {name} \
  --restart unless-stopped \
  --entrypoint sh \
  -v {runner_dir}:/etc/gitlab-runner \
  -v {docker_socket}:/var/run/docker.sock \
  -v {cache_dir}:/cache \
  -v {pool_cache}:/pool-cache \
  --label jeryu.managed=true \
  --label jeryu.pool={pool_name_q} \
  --label jeryu.manager_id={manager_id_q} \
  --label jeryu.node_alias={alias_q} \
  {image} \
  -lc {bootstrap}",
            name = shell_quote(&container_name),
            runner_dir = shell_quote(&runner_dir),
            docker_socket = shell_quote(&self.node.docker_socket),
            cache_dir = shell_quote(&cache_dir),
            pool_cache = shell_quote(&pool_cache),
            pool_name_q = shell_quote(pool_name),
            manager_id_q = shell_quote(manager_id),
            alias_q = shell_quote(&self.node.alias),
            image = shell_quote(runner_image),
            bootstrap = shell_quote(&bootstrap_cmd),
        );
        run_remote_shell(&cfg, &docker_run, false)
            .await
            .context("starting runner container on remote node")?;

        // Step 4: get container ID.
        let inspect_script = format!(
            "docker inspect --format='{{{{.Id}}}}' {}",
            shell_quote(&container_name)
        );
        let container_id = match run_remote_shell_capture(&cfg, &inspect_script)
            .await
            .context("inspecting remote container")?
        {
            Some(id) if !id.trim().is_empty() => id.trim().to_string(),
            _ => bail!(
                "remote container '{}' started but no ID returned from docker inspect",
                container_name
            ),
        };

        info!(
            node = %self.node.alias,
            manager_id,
            container_id = %container_id,
            pool = pool_name,
            "started remote runner manager"
        );

        Ok(ManagerHandle {
            backend_id: container_id,
            config_ref: runner_dir,
        })
    }

    /// Stop a remote manager with graceful SIGQUIT drain.
    ///
    /// Each SSH step is best-effort: the container may already be stopped or
    /// the node may be unreachable. Failures are logged at DEBUG level and
    /// the function always returns `Ok(())` so the caller can continue
    /// cleaning up DB state.
    async fn stop_manager(&self, backend_id: &str, drain_timeout_secs: i64) -> Result<()> {
        let cfg = self.remote_config();
        let alias = &self.node.alias;

        let sigquit = format!(
            "docker kill --signal SIGQUIT {} 2>/dev/null || true",
            shell_quote(backend_id)
        );
        if let Err(e) = run_remote_shell(&cfg, &sigquit, true).await {
            debug!(node = %alias, backend_id, error = %e, "SIGQUIT send skipped (node unreachable or container already stopped)");
        }

        let stop = format!(
            "docker stop --time {} {} 2>/dev/null || true",
            drain_timeout_secs,
            shell_quote(backend_id)
        );
        if let Err(e) = run_remote_shell(&cfg, &stop, true).await {
            debug!(node = %alias, backend_id, error = %e, "docker stop skipped");
        }

        let rm = format!(
            "docker rm -f {} 2>/dev/null || true",
            shell_quote(backend_id)
        );
        if let Err(e) = run_remote_shell(&cfg, &rm, true).await {
            debug!(node = %alias, backend_id, error = %e, "docker rm skipped");
        }

        Ok(())
    }

    /// List all running container IDs for jeryu-managed containers on this node.
    async fn list_running_backend_ids(&self) -> Result<BTreeSet<String>> {
        let cfg = self.remote_config();
        let script = "docker ps --filter label=jeryu.managed=true --format '{{.ID}}' 2>/dev/null";
        match run_remote_shell_capture(&cfg, script).await {
            Ok(Some(output)) => {
                let ids: BTreeSet<String> = output
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                Ok(ids)
            }
            Ok(None) | Err(_) => {
                warn!(node = %self.node.alias, "SSH unreachable during list_running_backend_ids");
                Err(anyhow::anyhow!("SSH to node '{}' failed", self.node.alias))
            }
        }
    }

    /// Fetch recent logs for a remote container.
    async fn get_manager_logs(&self, backend_id: &str, lines: usize) -> Result<String> {
        let cfg = self.remote_config();
        let script = format!(
            "docker logs --tail {} {} 2>&1",
            lines,
            shell_quote(backend_id)
        );
        match run_remote_shell_capture(&cfg, &script).await? {
            Some(output) => Ok(output),
            None => Ok(String::new()), // container produced no output
        }
    }

    /// Reload runner config via SIGHUP after token rotation.
    async fn reload_manager_config(&self, backend_id: &str) -> Result<()> {
        let cfg = self.remote_config();
        let alias = &self.node.alias;
        let script = format!(
            "docker kill --signal HUP {} 2>/dev/null || true",
            shell_quote(backend_id)
        );
        if let Err(e) = run_remote_shell(&cfg, &script, true).await {
            debug!(node = %alias, backend_id, error = %e, "SIGHUP send skipped (node unreachable or container already stopped)");
        }
        Ok(())
    }

    /// Clean up orphaned manager directories and prune old Docker images.
    async fn gc_storage(&self, max_gib: f64) -> Result<()> {
        let cfg = self.remote_config();

        let used_kb = get_remote_used_kb(&cfg, &self.node.runner_cache_dir).await?;
        let used_gib = used_kb as f64 / (1024.0 * 1024.0);

        debug!(node = %self.node.alias, used_gib, max_gib, "node storage check");

        if used_gib < max_gib * 0.9 {
            return Ok(());
        }

        warn!(node = %self.node.alias, used_gib, max_gib, "node storage above 90%; running GC");

        let prune = "docker image prune -f --filter 'until=168h' 2>/dev/null || true";
        if let Err(e) = run_remote_shell(&cfg, prune, true).await {
            debug!(node = %self.node.alias, error = %e, "docker image prune skipped (node unreachable)");
        }

        let used_kb_after = get_remote_used_kb(&cfg, &self.node.runner_cache_dir).await?;
        let used_gib_after = used_kb_after as f64 / (1024.0 * 1024.0);

        if used_gib_after >= max_gib * 0.95 {
            warn!(
                node = %self.node.alias,
                used_gib = used_gib_after,
                max_gib,
                "node storage still critical after GC; manual intervention may be needed"
            );
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_types::NodeConfig;

    #[test]
    fn backend_label() {
        let node = NodeConfig::new("xbabe0".to_string(), "deploy@xbabe0".to_string());
        let backend = RemoteDockerBackend::new(node);
        assert_eq!(backend.backend_label(), "docker-remote");
    }

    #[test]
    fn runner_dir_uses_node_data_dir() {
        let node = NodeConfig::new("xbabe0".to_string(), "u@h".to_string());
        let backend = RemoteDockerBackend::new(node);
        let dir = backend.runner_dir("abc-123");
        assert!(dir.contains("abc-123"));
        assert!(dir.starts_with("~/.jeryu/runners"));
    }

    #[test]
    fn gitlab_url_uses_override_when_set() {
        let mut node = NodeConfig::new("n0".to_string(), "u@h".to_string());
        node.gitlab_url_override = Some("http://192.168.1.100:8929".to_string());
        let backend = RemoteDockerBackend::new(node);
        assert_eq!(backend.gitlab_url(), "http://192.168.1.100:8929");
    }

    #[test]
    fn gitlab_url_defaults_to_gitlab_local() {
        let node = NodeConfig::new("n0".to_string(), "u@h".to_string());
        let backend = RemoteDockerBackend::new(node);
        let url = backend.gitlab_url();
        assert!(url.contains("gitlab.local") || url.contains("localhost"));
    }
}
