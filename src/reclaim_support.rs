use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{info, warn};

pub(crate) async fn print_cmd(label: &str, cmd: &mut Command) -> Result<()> {
    println!("\n== {} ==", label);
    let output = cmd.output().await?;
    if output.status.success() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    } else {
        warn!(
            "{} failed: {}",
            label,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(crate) async fn run_docker_prune(args: &[&str]) -> Result<()> {
    info!("Running: docker {}", args.join(" "));
    let output = Command::new("docker").args(args).output().await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("Total reclaimed space") || line.contains("Deleted") {
                println!("{}", line);
            }
        }
    } else {
        warn!(
            "Failed to run docker {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(crate) async fn truncate_gitlab_logs() -> Result<()> {
    let logs_dir = crate::config::gitlab_logs_dir();
    let script = format!(
        r#"
set -eu
find "{logs}" -type f \( -name '@*' -o -name '*.gz' \) -exec rm -f {{}} + || true
find "{logs}" -type f -name current -exec sh -c ': > "$1"' _ {{}} \; || true
find "{logs}/gitlab-rails" -type f \( -name '*_json.log' -o -name '*_client.log' \) -exec sh -c ': > "$1"' _ {{}} \; || true
"#,
        logs = logs_dir.display()
    );
    let output = Command::new("sh")
        .arg("-lc")
        .arg(script)
        .output()
        .await
        .context("truncating gitlab logs")?;
    if !output.status.success() {
        warn!(
            "gitlab log truncation warning: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

pub(crate) async fn truncate_docker_json_logs() -> Result<()> {
    let script = r#"
set -eu
for cid in $(docker ps -aq --filter name=jeryu-gitlab --filter label=jeryu.managed=true); do
  log_path=$(docker inspect --format '{{.LogPath}}' "$cid" 2>/dev/null || true)
  if [ -n "$log_path" ] && [ -f "$log_path" ]; then
    : > "$log_path" || true
  fi
done
"#;
    let output = Command::new("sh")
        .arg("-lc")
        .arg(script)
        .output()
        .await
        .context("truncating docker json logs")?;
    if !output.status.success() {
        warn!(
            "docker json log truncation warning: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

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

pub(crate) async fn evict_artifacts_over_budget(
    dir: &Path,
    suffix: &str,
    budget_bytes: u64,
) -> u64 {
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    walk_files_with_suffix(dir, suffix, |path, meta| {
        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        files.push((path, meta.len(), mtime));
    })
    .await;

    let total: u64 = files.iter().map(|(_, sz, _)| sz).sum();
    if total <= budget_bytes {
        return 0;
    }

    files.sort_by_key(|(_, _, mtime)| *mtime);
    let mut to_free = total - budget_bytes;
    let mut removed = 0u64;
    for (path, size, _) in files {
        if to_free == 0 {
            break;
        }
        if let Err(e) = tokio::fs::remove_file(&path).await {
            warn!(path = %path.display(), error = %e, "artifact eviction failed");
        } else {
            to_free = to_free.saturating_sub(size);
            removed += 1;
        }
    }
    removed
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

pub(crate) async fn sweep_stale_files(
    dir: &Path,
    suffix: &str,
    max_age: std::time::Duration,
) -> u64 {
    let mut removed = 0u64;
    let mut to_remove: Vec<PathBuf> = Vec::new();
    walk_files_with_suffix(dir, suffix, |path, meta| {
        let is_stale = meta
            .modified()
            .ok()
            .and_then(|mtime| std::time::SystemTime::now().duration_since(mtime).ok())
            .is_some_and(|age| age >= max_age);
        if is_stale {
            to_remove.push(path);
        }
    })
    .await;

    for path in to_remove {
        if let Err(e) = tokio::fs::remove_file(&path).await {
            warn!(path = %path.display(), error = %e, "failed to remove outdated artifact");
        } else {
            removed += 1;
        }
    }
    removed
}

async fn walk_files_with_suffix<F>(dir: &Path, suffix: &str, mut visit: F)
where
    F: FnMut(PathBuf, std::fs::Metadata),
{
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
                continue;
            }
            let name = entry.file_name();
            if !name.to_string_lossy().ends_with(suffix) {
                continue;
            }
            visit(entry.path(), meta);
        }
    }
}

// ---------------------------------------------------------------------------
// Process helpers (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "reclaim_support_proc.rs"]
mod reclaim_support_proc;
pub use reclaim_support_proc::*;
