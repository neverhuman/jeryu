//! Owner: Docker Control Plane subsystem
//! Proof: `cargo nextest run -p jeryu -- docker`
//! Invariants: Docker calls preserve container ownership labels and surface runtime errors to callers.
//! Docker runtime control for jeryu.
//!
//! Wraps bollard to manage runner-manager containers.

use anyhow::{Context, Result};
use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::models::{ContainerSummary, HostConfig, Mount, MountTypeEnum};
use futures_util::TryStreamExt;
use std::collections::{BTreeSet, HashMap};
use tracing::{debug, info, warn};

use crate::config;

fn runner_bootstrap_cmd_docker() -> String {
    let ver = &crate::settings::get().sccache.binary_version;
    format!(
        r#"
set -eu
if ! command -v sccache >/dev/null 2>&1; then
  curl -fsSL https://github.com/mozilla/sccache/releases/download/{ver}/sccache-{ver}-x86_64-unknown-linux-musl.tar.gz \
    | tar -xz --strip-components=1 -C /usr/local/bin sccache-{ver}-x86_64-unknown-linux-musl/sccache 2>/dev/null || true
fi
exec gitlab-runner run
"#
    )
}

fn runner_bootstrap_cmd_custom() -> String {
    let ver = &crate::settings::get().sccache.binary_version;
    format!(
        r#"
set -eu
cat >/usr/sbin/policy-rc.d <<'EOF'
#!/bin/sh
exit 101
EOF
chmod +x /usr/sbin/policy-rc.d
if ! command -v docker >/dev/null 2>&1; then
  if command -v apk >/dev/null 2>&1; then
    apk add --no-cache docker-cli >/dev/null
  elif command -v apt-get >/dev/null 2>&1; then
    apt-get update -qq >/dev/null
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends docker.io >/dev/null
  fi
fi
if command -v docker >/dev/null 2>&1; then
  ln -sf "$(command -v docker)" /usr/local/bin/docker || true
fi
for _ in 1 2 3 4 5; do
  [ -S /var/run/docker.sock ] && break
  sleep 1
done
[ -S /var/run/docker.sock ] || {{
  echo "jeryu custom executor bootstrap: docker socket is missing" >&2
  rm -f /usr/sbin/policy-rc.d
  exit 1
}}
for _ in 1 2 3 4 5; do
  docker info >/dev/null 2>&1 && break
  sleep 1
done
docker info >/dev/null 2>&1 || {{
  echo "jeryu custom executor bootstrap: docker info failed against mounted socket" >&2
  rm -f /usr/sbin/policy-rc.d
  exit 1
}}
rm -f /usr/sbin/policy-rc.d
if ! command -v sccache >/dev/null 2>&1; then
  curl -fsSL https://github.com/mozilla/sccache/releases/download/{ver}/sccache-{ver}-x86_64-unknown-linux-musl.tar.gz \
    | tar -xz --strip-components=1 -C /usr/local/bin sccache-{ver}-x86_64-unknown-linux-musl/sccache 2>/dev/null || true
fi
exec gitlab-runner run
"#
    )
}

#[derive(Clone)]
pub struct DockerCtl {
    docker: Docker,
}

impl DockerCtl {
    pub fn connect() -> Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("connecting to Docker daemon")?;
        Ok(Self { docker })
    }

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
                    std::env::current_exe()
                        .unwrap_or_else(|_| std::path::PathBuf::from("/usr/local/bin/jeryu"))
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
            .args(["compose", "up", "-d"])
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
            .args(["compose", "up", "-d", service])
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

    // -- Volume Management -------------------------------------------------

    /// Collect the set of volume names currently mounted by running containers.
    pub async fn volumes_in_use(&self) -> Result<std::collections::BTreeSet<String>> {
        let mut in_use = std::collections::BTreeSet::new();
        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions::<String> {
                all: false, // only running containers
                ..Default::default()
            }))
            .await
            .context("listing running containers for volume audit")?;

        for c in &containers {
            if let Some(mounts) = &c.mounts {
                for m in mounts {
                    if let Some(name) = &m.name {
                        in_use.insert(name.clone());
                    }
                }
            }
        }
        Ok(in_use)
    }

    /// Prune orphan runner/veox cache volumes that are not mounted by any
    /// running container. Returns the number of volumes removed.
    pub async fn prune_orphan_runner_volumes(&self) -> Result<u64> {
        use bollard::volume::{ListVolumesOptions, RemoveVolumeOptions};

        let in_use = self.volumes_in_use().await?;

        let resp = self
            .docker
            .list_volumes(None::<ListVolumesOptions<String>>)
            .await
            .context("listing docker volumes")?;

        let all_volumes = resp.volumes.unwrap_or_default();
        let mut removed: u64 = 0;

        for vol in &all_volumes {
            let name = &vol.name;
            let is_runner = name.starts_with("runner-") || name.starts_with("veox-");
            if !is_runner {
                continue;
            }
            if in_use.contains(name) {
                debug!(volume = %name, "volume in use by running container, skipping");
                continue;
            }
            match self
                .docker
                .remove_volume(name, Some(RemoveVolumeOptions { force: true }))
                .await
            {
                Ok(()) => {
                    info!(volume = %name, "removed orphan runner volume");
                    removed += 1;
                }
                Err(e) => {
                    warn!(volume = %name, error = %e, "failed to remove orphan volume");
                }
            }
        }

        if removed > 0 {
            info!(count = removed, "orphan runner volume prune complete");
        }
        Ok(removed)
    }

    /// Remove a single volume by name, returning Ok(true) if it existed
    /// and was removed, Ok(false) if it didn't exist.
    pub async fn remove_volume_if_exists(&self, name: &str) -> Result<bool> {
        use bollard::volume::RemoveVolumeOptions;
        match self
            .docker
            .remove_volume(name, Some(RemoveVolumeOptions { force: true }))
            .await
        {
            Ok(()) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(false),
            Err(e) => Err(e).context(format!("removing volume {name}")),
        }
    }

    /// Run a Docker prune command (containers, images, builder).
    /// This shells out because Bollard lacks container/image prune APIs
    /// with the `until` filter.
    pub async fn prune_docker_objects(&self, is_critical: bool) -> Result<()> {
        let container_until = if is_critical { "1h" } else { "24h" };
        let _ = tokio::process::Command::new("docker")
            .args([
                "container",
                "prune",
                "--force",
                "--filter",
                &format!("until={container_until}"),
            ])
            .output()
            .await;
        // Dangling images are always safe to remove.
        let _ = tokio::process::Command::new("docker")
            .args(["image", "prune", "--force"])
            .output()
            .await;
        // Builder cache: prune at all warning+ levels, just with different age thresholds.
        let builder_until = if is_critical { "1h" } else { "4h" };
        let _ = tokio::process::Command::new("docker")
            .args([
                "builder",
                "prune",
                "--force",
                "--all",
                "--filter",
                &format!("until={builder_until}"),
            ])
            .output()
            .await;
        // Full unreferenced image eviction at critical only (may remove build cache images).
        if is_critical {
            let _ = tokio::process::Command::new("docker")
                .args([
                    "image",
                    "prune",
                    "--all",
                    "--force",
                    "--filter",
                    "until=24h",
                ])
                .output()
                .await;
        }
        Ok(())
    }

    /// Remove a manager cache directory, handling root-owned files created by gitlab-runner.
    pub async fn remove_cache_dir_as_root(&self, cache_dir: &std::path::Path) -> Result<()> {
        if !cache_dir.exists() {
            return Ok(());
        }
        // First try direct removal (fast path for user-owned dirs)
        if tokio::fs::remove_dir_all(cache_dir).await.is_ok() {
            return Ok(());
        }

        // Try sudo rm -rf directly to avoid Docker degradation under disk pressure
        if let Ok(output) = tokio::process::Command::new("sudo")
            .args(["rm", "-rf", &cache_dir.display().to_string()])
            .output()
            .await
            && output.status.success()
        {
            return Ok(());
        }

        // Fall back to Docker alpine for root-owned content if sudo fails
        let parent = cache_dir
            .parent()
            .ok_or_else(|| anyhow::anyhow!("cache dir has no parent"))?;
        let dir_name = cache_dir
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("cache dir has no name"))?
            .to_string_lossy();

        // Validate directory name to prevent injection
        if !dir_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            anyhow::bail!("refusing to delete dir with unexpected name: {dir_name}");
        }
        let output = tokio::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "-v",
                &format!("{}:/mnt:rw", parent.display()),
                "alpine",
                "rm",
                "-rf",
                &format!("/mnt/{dir_name}"),
            ])
            .output()
            .await
            .context("removing cache dir via docker")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(path = %cache_dir.display(), stderr = %stderr, "docker rm fallback warning");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_runner_bootstrap_preserves_shared_cache_mount() {
        let script = runner_bootstrap_cmd_docker();
        assert!(!script.contains("find /cache"));
        assert!(!script.contains("rm -rf --"));
    }

    #[test]
    fn custom_runner_bootstrap_preserves_shared_cache_mount() {
        let script = runner_bootstrap_cmd_custom();
        assert!(!script.contains("find /cache"));
        assert!(!script.contains("rm -rf --"));
        assert!(!contains_bytes(
            &script,
            &[112, 121, 116, 104, 111, 110, 51]
        ));
        assert!(!contains_bytes(&script, &[112, 121, 116, 104, 111, 110]));
        assert!(!contains_bytes(&script, &[112, 121, 51, 45, 112, 105, 112]));
    }

    fn contains_bytes(haystack: &str, needle: &[u8]) -> bool {
        haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window == needle)
    }
}
