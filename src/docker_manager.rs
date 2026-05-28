//! Owner: Docker Control Plane subsystem
//! Proof: `cargo nextest run -p jeryu -- docker`
//! Invariants: Docker calls preserve container ownership labels and surface runtime errors to callers.

use anyhow::{Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions, UpdateContainerOptions,
};
use bollard::models::{
    ContainerSummary, HostConfig, Mount, MountTypeEnum, RestartPolicy, RestartPolicyNameEnum,
};
use futures_util::TryStreamExt;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use super::DockerCtl;
use crate::config;

#[path = "docker_manager_support.rs"]
mod docker_manager_support;
use docker_manager_support::{
    compose_down, compose_up, compose_up_service, current_exe_mount_source,
    runner_bootstrap_cmd_custom, runner_bootstrap_cmd_docker,
};

#[path = "docker_manager_ops.rs"]
mod docker_manager_ops;

fn compose_up_targets() -> [&'static str; 2] {
    ["gitlab", "vault"]
}

pub struct RunnerManagerStartSpec<'a> {
    pub manager_id: &'a str,
    pub pool_name: &'a str,
    pub config_dir: &'a str,
    pub manager_cache_dir: &'a str,
    pub pool_cache_dir: &'a str,
    pub executor: &'a str,
    pub docker_socket: Option<&'a str>,
}

fn runner_restart_policy() -> RestartPolicy {
    RestartPolicy {
        name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
        maximum_retry_count: Some(0),
    }
}

impl DockerCtl {
    /// Start a new runner-manager container for a pool.
    /// Returns the Docker container ID.
    pub async fn start_runner_manager(&self, spec: RunnerManagerStartSpec<'_>) -> Result<String> {
        let RunnerManagerStartSpec {
            manager_id,
            pool_name,
            config_dir,
            manager_cache_dir,
            pool_cache_dir,
            executor,
            docker_socket,
        } = spec;
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
            network_mode: Some(config::LOCAL_DOCKER_NETWORK_NAME.to_string()),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            restart_policy: Some(runner_restart_policy()),
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
                ("jeryu.pool".to_string(), pool_name.to_string()),
                ("jeryu.manager_id".to_string(), manager_id.to_string()),
                (
                    "jeryu.node_alias".to_string(),
                    config::LOCAL_NODE_ALIAS.to_string(),
                ),
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

    /// Ensure an existing local runner-manager survives Docker daemon or process restarts.
    pub async fn ensure_runner_manager_restart_policy(&self, container_id: &str) -> Result<()> {
        self.docker
            .update_container(
                container_id,
                UpdateContainerOptions::<String> {
                    restart_policy: Some(runner_restart_policy()),
                    ..Default::default()
                },
            )
            .await
            .with_context(|| format!("updating runner restart policy for {container_id}"))?;
        Ok(())
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
}

#[cfg(test)]
mod docker_manager_tests;
