//! Owner: Docker Control Plane subsystem
//! Proof: `cargo nextest run -p jeryu -- docker`
//! Invariants: Docker calls preserve container ownership labels and surface runtime errors to callers.

use anyhow::{Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::models::{
    ContainerSummary, HostConfig, HostConfigLogConfig, Mount, MountTypeEnum, ResourcesUlimits,
    RestartPolicy, RestartPolicyNameEnum,
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

const RUNNER_MEMORY_BYTES: i64 = 8 * 1024 * 1024 * 1024;
const RUNNER_NANO_CPUS: i64 = 4_000_000_000;
const RUNNER_NOFILE_LIMIT: i64 = 65_536;

fn runner_manager_host_config(mounts: Vec<Mount>) -> HostConfig {
    HostConfig {
        mounts: Some(mounts),
        extra_hosts: Some(vec![format!("{}:host-gateway", config::GITLAB_HOSTNAME)]),
        log_config: Some(HostConfigLogConfig {
            typ: Some("json-file".to_string()),
            config: Some(HashMap::from([
                ("max-size".to_string(), "50m".to_string()),
                ("max-file".to_string(), "3".to_string()),
            ])),
        }),
        memory: Some(RUNNER_MEMORY_BYTES),
        memory_swap: Some(RUNNER_MEMORY_BYTES),
        nano_cpus: Some(RUNNER_NANO_CPUS),
        ulimits: Some(vec![ResourcesUlimits {
            name: Some("nofile".to_string()),
            soft: Some(RUNNER_NOFILE_LIMIT),
            hard: Some(RUNNER_NOFILE_LIMIT),
        }]),
        restart_policy: Some(RestartPolicy {
            name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
            maximum_retry_count: None,
        }),
        ..Default::default()
    }
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

        let host_config = runner_manager_host_config(mounts);

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
}

#[cfg(test)]
mod docker_manager_tests;
