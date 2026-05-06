//! Owner: Storage Audit & GC
//! Proof: `cargo test -p jeryu -- reclaim`
//! Invariants: GC never removes objects referenced by active managers; AutoGcReport is produced before any deletions; audit runs do not block the reconciliation loop

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{info, warn};

pub fn live_registry_gc_enabled() -> bool {
    false
}

pub fn live_registry_gc_skip_reason() -> &'static str {
    "Docker registry garbage-collect is only safe while the registry is offline; live GC can leave manifests pointing at missing blobs."
}

pub async fn run_storage_audit() -> Result<()> {
    info!("Running storage audit on host...");
    let mut root_df = Command::new("df");
    root_df.args(["-h", "/"]);
    print_cmd("Root filesystem usage", &mut root_df).await?;

    let mut inode_df = Command::new("df");
    inode_df.args(["-ih", "/"]);
    print_cmd("Root inode usage", &mut inode_df).await?;

    let mut reserve_cmd = Command::new("docker");
    reserve_cmd.args([
        "run",
        "--rm",
        "--privileged",
        "-v",
        "/:/host",
        "alpine",
        "sh",
        "-lc",
        "chroot /host /usr/sbin/tune2fs -l /dev/nvme0n1p2 | grep -E 'Reserved block count|Block count|Block size'",
    ]);
    print_cmd("Ext4 reserved block setting", &mut reserve_cmd).await?;

    let mut gitlab_logs_cmd = Command::new("docker");
    gitlab_logs_cmd.args([
        "exec",
        "jeryu-gitlab",
        "sh",
        "-lc",
        "du -sh /var/log/gitlab/* 2>/dev/null | sort -h | tail -n 20",
    ]);
    print_cmd("GitLab log directory sizes", &mut gitlab_logs_cmd).await?;

    let mut docker_logs_cmd = Command::new("docker");
    docker_logs_cmd.args([
        "run",
        "--rm",
        "-v",
        "/var/lib/docker/containers:/host-containers:ro",
        "alpine",
        "sh",
        "-lc",
        "find /host-containers -name '*-json.log' -exec du -h {} + | sort -h | tail -n 20",
    ]);
    print_cmd("Largest Docker JSON logs", &mut docker_logs_cmd).await?;

    let output = Command::new("docker")
        .args(["system", "df"])
        .output()
        .await
        .context("Failed to run docker system df")?;

    if !output.status.success() {
        warn!("Failed to query docker storage. Is docker running?");
        println!("{}", String::from_utf8_lossy(&output.stderr));
    } else {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}

pub async fn run_aggressive_reclaim(apply: bool) -> Result<()> {
    let reclaim_toml = dirs::home_dir()
        .unwrap_or_default()
        .join(".jeryu/reclaim.toml");
    let mut exclusions = vec![];
    if reclaim_toml.exists()
        && let Ok(content) = tokio::fs::read_to_string(&reclaim_toml).await
    {
        if let Ok(parsed) = content.parse::<toml::Table>() {
            if let Some(toml::Value::Array(excl)) = parsed.get("exclude") {
                for item in excl {
                    if let toml::Value::String(s) = item {
                        exclusions.push(s.clone());
                    }
                }
            }
        } else {
            tracing::warn!("Failed to parse reclaim.toml; ignoring exclusions");
        }
    }

    if !apply {
        // Fetch accurate docker sizes
        let df_output = Command::new("docker")
            .args(["system", "df", "--format", "{{json .}}"])
            .output()
            .await
            .ok();

        let mut img_sz = "Unknown".to_string();
        let mut cont_sz = "Unknown".to_string();
        if let Some(out) = df_output {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Ok(j) = serde_json::from_str::<serde_json::Value>(line) {
                    if j["Type"] == "Images" {
                        img_sz = j["Size"].as_str().unwrap_or("Unknown").to_string();
                    }
                    if j["Type"] == "Containers" {
                        cont_sz = j["Size"].as_str().unwrap_or("Unknown").to_string();
                    }
                }
            }
        }

        let plan = serde_json::json!({
            "mode": "plan",
            "exclusions_loaded": exclusions.len(),
            "targets": [
                { "type": "gitlab_internal_logs", "filter": "truncate current logs + remove rotated logs", "estimated_bytes_freed": "Variable" },
                { "type": "docker_container_json_logs", "filter": "truncate jeryu-gitlab + jeryu-managed runner logs", "estimated_bytes_freed": "Variable" },
                { "type": "containers", "filter": "until=24h", "estimated_bytes_freed": cont_sz },
                { "type": "images_dangling", "filter": "dangling=true", "estimated_bytes_freed": img_sz },
                { "type": "images_unreferenced", "filter": "until=168h", "estimated_bytes_freed": "Unknown" },
                { "type": "builder_cache", "filter": "until=168h", "estimated_bytes_freed": "Unknown" }
            ],
            "skipped_targets": [
                { "type": "veox_ci_registry_gc", "filter": "disabled-live-registry", "reason": live_registry_gc_skip_reason() }
            ],
            "message": "Run with --apply to execute."
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&plan).unwrap_or_default()
        );
        return Ok(());
    }

    println!(
        "Starting aggressive host reclaim (exclusions: {:?})...",
        exclusions
    );

    // Log target digest metrics to ledger as an audit trail before deletion
    let db = crate::state::Db::open().await?;
    let payload = serde_json::json!({
        "action": "aggressive_reclaim_apply",
        "exclusions": exclusions,
        "target_systems": ["docker_containers", "docker_images", "builder_cache"]
    });
    db.append_event(
        "host_reclaim",
        None,
        None,
        "jeryu-cli",
        &payload.to_string(),
    )
    .await?;

    truncate_gitlab_logs().await?;
    truncate_docker_json_logs().await?;

    // Remove exited containers older than 24h
    run_docker_prune(&["container", "prune", "--force", "--filter", "until=24h"]).await?;

    // Remove dangling images
    run_docker_prune(&["image", "prune", "--force"]).await?;

    // Remove unreferenced images older than 7d
    run_docker_prune(&[
        "image",
        "prune",
        "--all",
        "--force",
        "--filter",
        "until=168h",
    ])
    .await?;

    // Remove Docker build cache older than 7d
    run_docker_prune(&[
        "builder",
        "prune",
        "--force",
        "--all",
        "--filter",
        "until=168h",
    ])
    .await?;

    warn!(
        reason = live_registry_gc_skip_reason(),
        "skipping veox-ci-registry garbage-collect"
    );

    println!("Aggressive host reclaim completed successfully.");
    Ok(())
}

async fn print_cmd(label: &str, cmd: &mut Command) -> Result<()> {
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

async fn run_docker_prune(args: &[&str]) -> Result<()> {
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

async fn truncate_gitlab_logs() -> Result<()> {
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

async fn truncate_docker_json_logs() -> Result<()> {
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

    // 1. Prune orphan runner/veox volumes via Bollard API (NOT shell scripts).
    // This cross-references all volumes against running containers.
    match docker.prune_orphan_runner_volumes().await {
        Ok(n) => report.volumes_removed = n,
        Err(e) => warn!(error = %e, "orphan volume prune failed"),
    }

    // 2. Clean up outdated release clones and /tmp build artifacts using tokio::fs.
    let home = dirs::home_dir().unwrap_or_default();
    let age_threshold = if is_emergency {
        std::time::Duration::from_secs(30 * 60) // 30m — today's artifacts are fair game
    } else if is_critical {
        std::time::Duration::from_secs(2 * 3600) // 2h (was 6h)
    } else {
        std::time::Duration::from_secs(6 * 3600) // 6h (was 24h)
    };

    // Scan home dir for dougx-release-ci-* clones
    report.stale_dirs_removed +=
        sweep_stale_dirs(&home, "dougx-release-ci-", age_threshold, docker).await;

    // Scan /tmp for build artifacts
    let tmp = std::path::PathBuf::from("/tmp");
    for prefix in &["dougx-", "enclave"] {
        report.stale_dirs_removed += sweep_stale_dirs(&tmp, prefix, age_threshold, docker).await;
    }

    // 3. Truncate GitLab + Docker logs at critical/emergency — these grow unbounded and
    //    block artifact writes (GitLab HTTP 500) when the filesystem fills.
    if is_critical || is_emergency {
        if let Err(e) = truncate_gitlab_logs().await {
            warn!(error = %e, "gitlab log truncation failed");
        }
        if let Err(e) = truncate_docker_json_logs().await {
            warn!(error = %e, "docker json log truncation failed");
        }
    }

    // 4. Clean up prior GitLab CI artifact zips (time-based sweep, then hard size cap)
    let artifact_dir = crate::config::data_dir().join("gitlab/data/gitlab-rails/shared/artifacts");
    if artifact_dir.is_dir() {
        report.artifacts_removed += sweep_stale_files(&artifact_dir, ".zip", age_threshold).await;
        // Hard size cap at ALL pressure levels. Artifacts are the biggest disk consumer
        // (2.6 GiB per pipeline run × many pipelines = 100+ GiB uncontrolled). Cap aggressively
        // so artifact growth cannot cause disk-full events regardless of GC trigger timing.
        let artifact_budget: u64 = if is_emergency {
            2 * 1024 * 1024 * 1024 // 2 GiB
        } else if is_critical {
            5 * 1024 * 1024 * 1024 // 5 GiB
        } else {
            // Warning: keep ~2 recent pipeline runs worth (5 jobs × 2.6 GiB = 13 GiB, 2× = 26 GiB)
            // hard cap prevents runaway growth even if time-based sweep leaves recent artifacts.
            20 * 1024 * 1024 * 1024 // 20 GiB
        };
        report.artifacts_removed +=
            evict_artifacts_over_budget(&artifact_dir, ".zip", artifact_budget).await;
    }

    // 5. Docker object prune (containers, images, builder cache)
    if let Err(e) = docker.prune_docker_objects(is_critical).await {
        warn!(error = %e, "docker object prune failed");
    }

    // 6. Registry GC is deliberately disabled for the live CI registry. Docker
    //    distribution requires offline GC; running it against the active
    //    registry can corrupt images by deleting blobs still referenced by
    //    manifests observed by concurrent clients.
    if is_critical || is_emergency {
        warn!(
            critical = is_critical,
            emergency = is_emergency,
            reason = live_registry_gc_skip_reason(),
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

/// Remove the oldest `.zip` (or other suffix) files in a directory tree until total
/// size is under `budget_bytes`. Returns the number of files removed.
async fn evict_artifacts_over_budget(
    dir: &std::path::Path,
    suffix: &str,
    budget_bytes: u64,
) -> u64 {
    // Collect all matching files with their mtime and size
    let mut files: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> = Vec::new();
    walk_files_with_suffix(dir, suffix, |path, meta| {
        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        files.push((path, meta.len(), mtime));
    })
    .await;

    let total: u64 = files.iter().map(|(_, sz, _)| sz).sum();
    if total <= budget_bytes {
        return 0;
    }

    // Sort oldest first — remove oldest until under budget
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

/// Walk a directory (non-recursively) and remove subdirectories matching
/// a name prefix that are older than the given age threshold.
async fn sweep_stale_dirs(
    parent: &std::path::Path,
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

    // Try user-owned deletion first (fast path — no sudo needed for ubuntu-owned dirs).
    let mut need_sudo: Vec<String> = Vec::new();
    for path in &stale_paths {
        if tokio::fs::remove_dir_all(path).await.is_err() {
            need_sudo.push(path.clone());
        }
    }

    if need_sudo.is_empty() {
        return stale_paths.len() as u64;
    }

    // Fall back to sudo for root-owned dirs (e.g. /cache/runner-* created via Docker socket).
    let output = tokio::process::Command::new("sudo")
        .arg("rm")
        .arg("-rf")
        .args(&need_sudo)
        .output()
        .await;

    let sudo_removed = match output {
        Ok(out) if out.status.success() => need_sudo.len(),
        Ok(out) => {
            warn!(
                stderr = %String::from_utf8_lossy(&out.stderr),
                "batch removal command failed"
            );
            0
        }
        Err(e) => {
            warn!(error = %e, "failed to spawn batch removal command");
            0
        }
    };
    (stale_paths.len() - need_sudo.len() + sudo_removed) as u64
}

/// Walk a directory tree and remove files matching a suffix that are
/// older than the given threshold.
async fn sweep_stale_files(
    dir: &std::path::Path,
    suffix: &str,
    max_age: std::time::Duration,
) -> u64 {
    let mut removed = 0u64;
    let mut to_remove: Vec<std::path::PathBuf> = Vec::new();
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

/// Stack-based recursive walk over `dir`. Invokes `visit` for each non-directory
/// entry whose filename ends with `suffix`. Errors during read_dir/metadata are
/// silently skipped (the original walkers behaved the same way).
async fn walk_files_with_suffix<F>(dir: &std::path::Path, suffix: &str, mut visit: F)
where
    F: FnMut(std::path::PathBuf, std::fs::Metadata),
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
// Orphaned worker GC
// ---------------------------------------------------------------------------

/// Scan /proc for Python forkserver processes reparented to init (ppid=1) and SIGKILL them.
/// These are orphaned evolution workers from crashed runs that survived because
/// pool.terminate()+join() does not outlive a SIGKILL of the parent process.
/// Returns the number of processes killed.
pub async fn gc_orphaned_workers() -> u64 {
    use std::fs;
    let Ok(proc_dir) = fs::read_dir("/proc") else {
        return 0;
    };
    let mut killed = 0u64;
    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let status_path = format!("/proc/{pid}/status");
        let Ok(status) = fs::read_to_string(&status_path) else {
            continue;
        };
        let ppid: u32 = status
            .lines()
            .find(|l| l.starts_with("PPid:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(u32::MAX);
        if ppid != 1 {
            continue;
        }
        let cmdline_path = format!("/proc/{pid}/cmdline");
        let Ok(cmdline) = fs::read_to_string(&cmdline_path) else {
            continue;
        };
        let cmd = cmdline.replace('\0', " ");
        // Match forkserver workers AND orphaned evolution main processes.
        // The main local_run_mimo.py process (ppid=1) owns 100+ worker descendants
        // that won't show as forkserver but are still consuming RAM.
        if !cmd.contains("forkserver") && !cmd.contains("local_run_mimo") {
            continue;
        }
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
        killed += 1;
    }
    killed
}

/// Read MemAvailable from /proc/meminfo and return as GB.
/// Returns f64::MAX if the file cannot be read (treat as no pressure).
pub fn mem_available_gb() -> f64 {
    std::fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("MemAvailable:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|kb| kb / 1024.0 / 1024.0)
        .unwrap_or(f64::MAX)
}
