use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use crate::exec::run_status_check;

pub async fn install_gc_timer(allow_sudo: bool) -> Result<i32> {
    let unit_dir = PathBuf::from("/etc/systemd/system");
    let service_src = repo_file("ops/ci/jeryu-gc.service");
    let timer_src = repo_file("ops/ci/jeryu-gc.timer");
    let service_dst = unit_dir.join("jeryu-gc.service");
    let timer_dst = unit_dir.join("jeryu-gc.timer");

    if !allow_sudo && !is_root() {
        bail!("install-gc-timer requires --allow-sudo unless run as root");
    }

    if is_root() {
        fs::copy(&service_src, &service_dst)
            .with_context(|| format!("copying {}", service_dst.display()))?;
        fs::copy(&timer_src, &timer_dst)
            .with_context(|| format!("copying {}", timer_dst.display()))?;
    } else {
        run_sudo_copy(&service_src, &service_dst).await?;
        run_sudo_copy(&timer_src, &timer_dst).await?;
    }

    run_systemctl(&["daemon-reload"]).await?;
    run_systemctl(&["enable", "--now", "jeryu-gc.timer"]).await?;
    run_systemctl(&["status", "jeryu-gc.timer", "--no-pager"]).await?;
    Ok(0)
}

/// Install and enable `jeryu-gcd.service` — the always-on disk daemon.
///
/// Returns Ok(0) on success. On systems without systemd (CI containers,
/// macOS, WSL without --user) this is a no-op when `systemctl` is missing,
/// rather than failing the surrounding bootstrap.
pub async fn install_gcd_service(allow_sudo: bool) -> Result<i32> {
    if !systemctl_available().await {
        eprintln!(
            "install-gcd-service: systemctl not found; skipping (set JERYU_BOOTSTRAP_SKIP_GCD=1 to silence)"
        );
        return Ok(0);
    }
    let unit_dir = PathBuf::from("/etc/systemd/system");
    let service_src = repo_file("ops/ci/jeryu-gcd.service");
    let service_dst = unit_dir.join("jeryu-gcd.service");

    if !allow_sudo && !is_root() {
        bail!("install-gcd-service requires --allow-sudo unless run as root");
    }
    if is_root() {
        fs::copy(&service_src, &service_dst)
            .with_context(|| format!("copying {}", service_dst.display()))?;
    } else {
        run_sudo_copy(&service_src, &service_dst).await?;
    }
    run_systemctl(&["daemon-reload"]).await?;
    run_systemctl(&["enable", "--now", "jeryu-gcd.service"]).await?;
    run_systemctl(&["status", "jeryu-gcd.service", "--no-pager"]).await?;
    Ok(0)
}

async fn systemctl_available() -> bool {
    Command::new("systemctl")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn repo_file(rel: &str) -> PathBuf {
    match std::env::current_dir() {
        Ok(mut dir) => loop {
            let candidate = dir.join(rel);
            if candidate.exists() {
                return candidate;
            }
            if !dir.pop() {
                return PathBuf::from(rel);
            }
        },
        Err(_) => PathBuf::from(rel),
    }
}

fn is_root() -> bool {
    // SAFETY: geteuid is a pure libc query with no aliasing or lifetime concerns.
    unsafe { libc::geteuid() == 0 }
}

async fn run_sudo_copy(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    let mut cmd = Command::new("sudo");
    cmd.args(["install", "-m", "0644"]).arg(src).arg(dst);
    run_status_check(&mut cmd, "copying systemd unit with sudo").await
}

async fn run_systemctl(args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("systemctl");
    cmd.args(args);
    run_status_check(&mut cmd, "running systemctl").await
}
