//! Owner: Storage Audit & GC
//! Proof: `cargo test -p jeryu -- reclaim`
//! Invariants: GC never removes objects referenced by active managers; AutoGcReport is produced before any deletions; audit runs do not block the reconciliation loop

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{info, warn};

#[path = "reclaim_support.rs"]
mod reclaim_support;

pub use reclaim_support::{AutoGcReport, gc_orphaned_workers, mem_available_gb, run_auto_gc};

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
    reclaim_support::print_cmd("Root filesystem usage", &mut root_df).await?;

    let mut inode_df = Command::new("df");
    inode_df.args(["-ih", "/"]);
    reclaim_support::print_cmd("Root inode usage", &mut inode_df).await?;

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
    reclaim_support::print_cmd("Ext4 reserved block setting", &mut reserve_cmd).await?;

    let mut gitlab_logs_cmd = Command::new("docker");
    gitlab_logs_cmd.args([
        "exec",
        "jeryu-gitlab",
        "sh",
        "-lc",
        "du -sh /var/log/gitlab/* 2>/dev/null | sort -h | tail -n 20",
    ]);
    reclaim_support::print_cmd("GitLab log directory sizes", &mut gitlab_logs_cmd).await?;

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
    reclaim_support::print_cmd("Largest Docker JSON logs", &mut docker_logs_cmd).await?;

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
    let reclaim_toml = match dirs::home_dir() {
        Some(home) => home.join(".jeryu/reclaim.toml"),
        None => std::path::PathBuf::from(".jeryu/reclaim.toml"),
    };
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
        let rendered_plan = match serde_json::to_string_pretty(&plan) {
            Ok(s) => s,
            Err(_) => String::new(),
        };
        println!("{}", rendered_plan);
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

    reclaim_support::truncate_gitlab_logs().await?;
    reclaim_support::truncate_docker_json_logs().await?;

    // Remove exited containers older than 24h
    reclaim_support::run_docker_prune(&["container", "prune", "--force", "--filter", "until=24h"])
        .await?;

    // Remove dangling images
    reclaim_support::run_docker_prune(&["image", "prune", "--force"]).await?;

    // Remove unreferenced images older than 7d
    reclaim_support::run_docker_prune(&[
        "image",
        "prune",
        "--all",
        "--force",
        "--filter",
        "until=168h",
    ])
    .await?;

    // Remove Docker build cache older than 7d
    reclaim_support::run_docker_prune(&[
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
