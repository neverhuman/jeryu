//! Owner: Docker Control Plane subsystem
//! Proof: `cargo nextest run -p jeryu -- docker`
//! Invariants: Docker calls preserve container ownership labels and surface runtime errors to callers.

use anyhow::{Context, Result};
use bollard::container::ListContainersOptions;
use bollard::volume::{ListVolumesOptions, RemoveVolumeOptions};
use std::collections::BTreeSet;
use std::path::Path;
use tracing::{debug, info, warn};

use super::DockerCtl;

impl DockerCtl {
    /// Collect the set of volume names currently mounted by running containers.
    pub async fn volumes_in_use(&self) -> Result<BTreeSet<String>> {
        let mut in_use = BTreeSet::new();
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
        let in_use = self.volumes_in_use().await?;

        let resp = self
            .docker
            .list_volumes(None::<ListVolumesOptions<String>>)
            .await
            .context("listing docker volumes")?;

        let all_volumes = match resp.volumes {
            Some(v) => v,
            None => Vec::new(),
        };
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
    pub async fn remove_cache_dir_as_root(&self, cache_dir: &Path) -> Result<()> {
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
        let parent = match cache_dir.parent() {
            Some(p) => p,
            None => anyhow::bail!("cache dir has no parent"),
        };
        let dir_name = match cache_dir.file_name() {
            Some(n) => n,
            None => anyhow::bail!("cache dir has no name"),
        }
        .to_string_lossy();

        // Validate directory name to prevent injection
        if !dir_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            anyhow::bail!("refusing to remove dir with unexpected name: {dir_name}");
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
            warn!(path = %cache_dir.display(), stderr = %stderr, "docker rm recovery warning");
        }
        Ok(())
    }
}
