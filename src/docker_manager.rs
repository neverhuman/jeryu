//! Owner: Docker Control Plane subsystem
//! Proof: `cargo nextest run -p jeryu -- docker`
//! Invariants: Docker calls preserve container ownership labels and surface runtime errors to callers.

use anyhow::{Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::models::{ContainerSummary, HostConfig, Mount, MountTypeEnum};
use futures_util::TryStreamExt;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use super::DockerCtl;
use crate::config;

#[path = "docker_manager_support.rs"]
mod docker_manager_support;
use docker_manager_support::{
    current_exe_mount_source, runner_bootstrap_cmd_custom, runner_bootstrap_cmd_docker,
};

fn compose_up_targets() -> [&'static str; 2] {
    ["gitlab", "vault"]
}

impl DockerCtl {
    /// Start a new runner-manager container for a pool.
    /// Returns the Docker container ID.
    pub async fn start_runner_manager(
        &self,
        manager_id: &str,
        config_dir: &str,
        manager_cache_dir: &str,
        pool_cache_dir: &str,
        executor: &str,
        docker_socket: Option<&str>,
    ) -> Result<String> {
        let container_name = format!("jeryu-runner-{}", manager_id);
        let socket = docker_socket.unwrap_or("/var/run/docker.sock");
        let bootstrap_cmd_owned = match executor {
            "custom" => runner_bootstrap_cmd_custom(),
            _ => runner_bootstrap_cmd_docker(),
        };
        let bootstrap_cmd = bootstrap_cmd_owned.as_str();

        let mounts = vec![
            Mount {
                target: Some("/etc/gitlab-runner".to_string()),
                source: Some(config_dir.to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(false),
                ..Default::default()
            },
            Mount {
                target: Some("/var/run/docker.sock".to_string()),
                source: Some(socket.to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(false),
                ..Default::default()
            },
            Mount {
                target: Some("/usr/local/bin/jeryu".to_string()),
                source: Some(
                    current_exe_mount_source(std::env::current_exe())
                        .to_string_lossy()
                        .to_string(),
                ),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(true),
                ..Default::default()
            },
            Mount {
                target: Some("/cache".to_string()),
                source: Some(manager_cache_dir.to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(false),
                ..Default::default()
            },
            Mount {
                target: Some("/pool-cache".to_string()),
                source: Some(pool_cache_dir.to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(false),
                ..Default::default()
            },
        ];

        let host_config = HostConfig {
            mounts: Some(mounts),
            extra_hosts: Some(vec![format!("{}:host-gateway", config::GITLAB_HOSTNAME)]),
            ..Default::default()
        };

        let container_config = Config {
            image: Some(config::GITLAB_RUNNER_IMAGE.to_string()),
            user: Some("root".to_string()),
            entrypoint: Some(vec!["sh".to_string(), "-lc".to_string()]),
            cmd: Some(vec![bootstrap_cmd.to_string()]),
            host_config: Some(host_config),
            labels: Some(HashMap::from([
                ("jeryu.managed".to_string(), "true".to_string()),
                ("jeryu.manager_id".to_string(), manager_id.to_string()),
            ])),
            ..Default::default()
        };

        let opts = CreateContainerOptions {
            name: &container_name,
            platform: None,
        };

        let resp = self
            .docker
            .create_container(Some(opts), container_config)
            .await
            .with_context(|| format!("creating runner container: {}", container_name))?;

        self.docker
            .start_container(&resp.id, None::<StartContainerOptions<String>>)
            .await
            .with_context(|| format!("starting runner container: {}", container_name))?;

        info!(container_id = %resp.id, name = %container_name, "started runner manager");
        Ok(resp.id)
    }

    /// Remove cached job state from a manager's bind-mounted /cache.
    pub async fn cleanup_runner_cache(&self, container_id: &str) -> Result<()> {
        // Runner managers share the host cache mount. Deleting /cache from one
        // manager can remove another active job's Cargo target directory
        // mid-compile, so cache eviction must stay in SmartCache/host GC where
        // active-manager preservation is enforced.
        debug!(
            container_id,
            "skipping destructive shared runner cache cleanup"
        );
        Ok(())
    }

    /// Drain a runner manager: SIGQUIT then wait for exit.
    pub async fn drain_runner_manager(&self, container_id: &str, timeout_secs: i64) -> Result<()> {
        info!(container_id, "sending SIGQUIT to drain runner manager");
        self.docker
            .kill_container(
                container_id,
                Some(KillContainerOptions { signal: "SIGQUIT" }),
            )
            .await
            .context("sending SIGQUIT to runner manager")?;

        debug!(container_id, timeout_secs, "waiting for runner to drain");
        let _ = self
            .docker
            .stop_container(container_id, Some(StopContainerOptions { t: timeout_secs }))
            .await;
        Ok(())
    }

    /// Force-stop a runner manager.
    pub async fn stop_runner_manager(&self, container_id: &str) -> Result<()> {
        self.docker
            .stop_container(container_id, Some(StopContainerOptions { t: 10 }))
            .await
            .context("stopping runner manager")?;
        info!(container_id, "stopped runner manager");
        Ok(())
    }

    /// Remove a stopped container.
    pub async fn remove_runner_manager(&self, container_id: &str) -> Result<()> {
        self.docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .context("removing runner container")?;
        info!(container_id, "removed runner container");
        Ok(())
    }

    /// SIGHUP for runner config hot-reload.
    pub async fn reload_runner_config(&self, container_id: &str) -> Result<()> {
        self.docker
            .kill_container(
                container_id,
                Some(KillContainerOptions { signal: "SIGHUP" }),
            )
            .await
            .context("sending SIGHUP to runner manager")?;
        debug!(container_id, "sent SIGHUP for config reload");
        Ok(())
    }

    /// Get recent logs from a manager container.
    pub async fn manager_logs(&self, container_id: &str, tail: usize) -> Result<Vec<String>> {
        let opts = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: tail.to_string(),
            ..Default::default()
        };

        let stream = self.docker.logs(container_id, Some(opts));
        let chunks: Vec<_> = stream.try_collect().await.context("reading runner logs")?;

        let lines: Vec<String> = chunks.iter().map(|c| c.to_string()).collect();
        Ok(lines)
    }

    /// List all jeryu-managed containers.
    pub async fn list_managed_containers(&self) -> Result<Vec<ContainerSummary>> {
        let mut filters = HashMap::new();
        filters.insert("label".to_string(), vec!["jeryu.managed=true".to_string()]);

        let opts = ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        };

        let containers = self
            .docker
            .list_containers(Some(opts))
            .await
            .context("listing managed containers")?;
        Ok(containers)
    }

    /// Return full Docker IDs for jeryu-managed runner containers that are actually running.
    pub async fn running_managed_container_ids(&self) -> Result<BTreeSet<String>> {
        let ids = self
            .list_managed_containers()
            .await?
            .into_iter()
            .filter(|container| container.state.as_deref() == Some("running"))
            .filter_map(|container| container.id)
            .collect();
        Ok(ids)
    }

    // -- Events ------------------------------------------------------------

    pub fn events(
        &self,
    ) -> impl futures_util::Stream<Item = Result<bollard::models::EventMessage, bollard::errors::Error>>
    {
        self.docker
            .events(None::<bollard::system::EventsOptions<String>>)
    }

    // -- Compose (shell out) -----------------------------------------------

    pub async fn compose_up(&self) -> Result<()> {
        let data_dir = config::data_dir();
        let output = tokio::process::Command::new("docker")
            .args(["compose", "up", "-d", "--no-deps"])
            .args(compose_up_targets())
            .current_dir(&data_dir)
            .output()
            .await
            .context("running docker compose up")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("docker compose up failed: {}", stderr);
        }
        info!("docker compose up completed");
        Ok(())
    }

    pub async fn compose_up_service(&self, service: &str) -> Result<()> {
        let data_dir = config::data_dir();
        let output = tokio::process::Command::new("docker")
            .args(["compose", "up", "-d", "--no-deps", service])
            .current_dir(&data_dir)
            .output()
            .await
            .with_context(|| format!("running docker compose up {service}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("docker compose up {service} failed: {}", stderr);
        }
        info!(service, "docker compose up service completed");
        Ok(())
    }

    pub async fn compose_down(&self) -> Result<()> {
        let data_dir = config::data_dir();
        let output = tokio::process::Command::new("docker")
            .args(["compose", "down"])
            .current_dir(&data_dir)
            .output()
            .await
            .context("running docker compose down")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("docker compose down warning: {}", stderr);
        }
        info!("docker compose down completed");
        Ok(())
    }
}

#[cfg(test)]
mod docker_manager_tests;
