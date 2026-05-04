use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

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

fn repo_file(rel: &str) -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        let candidate = dir.join(rel);
        if candidate.exists() {
            return candidate;
        }
        if !dir.pop() {
            return PathBuf::from(rel);
        }
    }
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

async fn run_sudo_copy(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    let status = Command::new("sudo")
        .args(["install", "-m", "0644"])
        .arg(src)
        .arg(dst)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .context("copying systemd unit with sudo")?;
    if !status.success() {
        bail!("sudo install failed");
    }
    Ok(())
}

async fn run_systemctl(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .context("running systemctl")?;
    if !status.success() {
        bail!("systemctl {:?} failed", args);
    }
    Ok(())
}
