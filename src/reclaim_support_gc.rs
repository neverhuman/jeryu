use super::*;
use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[path = "reclaim_support_files.rs"]
mod files;
pub(crate) use files::{evict_artifacts_over_budget, sweep_stale_files};

/// Report from an automatic GC cycle.
#[derive(Debug, Default)]
pub struct AutoGcReport {
    pub volumes_removed: u64,
    pub stale_dirs_removed: u64,
    pub artifacts_removed: u64,
}

pub async fn run_auto_gc(
    docker: &crate::docker::DockerCtl,
    is_critical: bool,
    is_emergency: bool,
) -> Result<AutoGcReport> {
    info!(
        critical = is_critical,
        emergency = is_emergency,
        "running automatic storage GC"
    );
    let mut report = AutoGcReport::default();

    if is_emergency {
        warn!(
            "disk pressure emergency: build/default pools should already be paused and draining before host GC"
        );
    }

    match docker.prune_orphan_runner_volumes().await {
        Ok(n) => report.volumes_removed = n,
        Err(e) => warn!(error = %e, "orphan volume prune failed"),
    }

    let home = match dirs::home_dir() {
        Some(home) => home,
        None => std::path::PathBuf::new(),
    };
    let age_threshold = if is_emergency {
        std::time::Duration::from_secs(30 * 60)
    } else if is_critical {
        std::time::Duration::from_secs(2 * 3600)
    } else {
        std::time::Duration::from_secs(6 * 3600)
    };

    report.stale_dirs_removed +=
        sweep_stale_dirs(&home, "dougx-release-ci-", age_threshold, docker).await;
    let tmp = std::path::PathBuf::from("/tmp");
    for prefix in &["dougx-", "enclave"] {
        report.stale_dirs_removed += sweep_stale_dirs(&tmp, prefix, age_threshold, docker).await;
    }

    if is_critical || is_emergency {
        if let Err(e) = truncate_gitlab_logs().await {
            warn!(error = %e, "gitlab log truncation failed");
        }
        if let Err(e) = truncate_docker_json_logs().await {
            warn!(error = %e, "docker json log truncation failed");
        }
    }

    let artifact_dir = crate::config::data_dir().join("gitlab/data/gitlab-rails/shared/artifacts");
    if artifact_dir.is_dir() {
        report.artifacts_removed += sweep_stale_files(&artifact_dir, ".zip", age_threshold).await;
        let artifact_budget: u64 = if is_emergency {
            2 * 1024 * 1024 * 1024
        } else if is_critical {
            5 * 1024 * 1024 * 1024
        } else {
            20 * 1024 * 1024 * 1024
        };
        report.artifacts_removed +=
            evict_artifacts_over_budget(&artifact_dir, ".zip", artifact_budget).await;
    }

    if let Err(e) = docker.prune_docker_objects(is_critical).await {
        warn!(error = %e, "docker object prune failed");
    }

    if is_critical || is_emergency {
        warn!(
            critical = is_critical,
            emergency = is_emergency,
            reason = crate::reclaim::live_registry_gc_skip_reason(),
            "skipping veox-ci-registry garbage-collect"
        );
    }

    info!(
        volumes = report.volumes_removed,
        dirs = report.stale_dirs_removed,
        artifacts = report.artifacts_removed,
        "automatic storage GC complete"
    );
    Ok(report)
}

pub(crate) async fn sweep_stale_dirs(
    parent: &Path,
    prefix: &str,
    max_age: std::time::Duration,
    _docker: &crate::docker::DockerCtl,
) -> u64 {
    let mut stale_paths = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(parent).await else {
        return 0;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with(prefix) {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        let is_stale = meta
            .modified()
            .ok()
            .and_then(|mtime| std::time::SystemTime::now().duration_since(mtime).ok())
            .is_some_and(|age| age >= max_age);

        if is_stale {
            stale_paths.push(entry.path().display().to_string());
        }
    }

    if stale_paths.is_empty() {
        return 0;
    }

    info!(
        count = stale_paths.len(),
        "batch removing outdated directories"
    );

    let mut need_sudo: Vec<String> = Vec::new();
    for path in &stale_paths {
        if tokio::fs::remove_dir_all(path).await.is_err() {
            need_sudo.push(path.clone());
        }
    }

    if need_sudo.is_empty() {
        return stale_paths.len() as u64;
    }

    let output = tokio::process::Command::new("sudo")
        .arg("rm")
        .arg("-rf")
        .args(&need_sudo)
        .output()
        .await;

    let sudo_removed = match output {
        Ok(out) if out.status.success() => need_sudo.len(),
        Ok(out) => {
            warn!(stderr = %String::from_utf8_lossy(&out.stderr), "batch removal command failed");
            0
        }
        Err(e) => {
            warn!(error = %e, "failed to spawn batch removal command");
            0
        }
    };
    (stale_paths.len() - need_sudo.len() + sudo_removed) as u64
}
