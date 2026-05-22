use anyhow::{Context, Result, bail};
use tokio::process::Command;

use super::*;

pub(crate) async fn run_remote_binary(
    cfg: &RemoteConfig,
    args: &[&str],
    allow_fail: bool,
) -> Result<Option<String>> {
    // Wrap in bash -lc so a tilde-prefixed remote_bin (e.g. ~/.jeryu/bin/jeryu)
    // gets word-split with tilde expansion. Without this, ssh hands
    // `~/.jeryu/bin/jeryu --version` to the remote sh as a literal command,
    // and POSIX sh only expands `~` at specific syntactic positions —
    // when not expanded, the binary path doesn't exist and exec fails.
    let script = format!(
        "{} {}",
        cfg.remote_bin,
        args.iter()
            .map(|a| shell_single_quote(a))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut cmd = ssh_bash_command(cfg, &script);
    let output = cmd.output().await.context("running remote binary")?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    } else if allow_fail {
        Ok(None)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "remote binary exited with {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }
}

pub(crate) async fn run_interactive_ssh(
    mut cmd: Command,
    _label: &'static str,
    context_msg: &'static str,
) -> Result<i32> {
    crate::exec::run_status_check(&mut cmd, context_msg).await?;
    Ok(0)
}

pub(crate) async fn run_remote_shell(
    cfg: &RemoteConfig,
    script: &str,
    allow_fail: bool,
) -> Result<()> {
    let output = capture_ssh_bash_output(cfg, script, "running remote shell").await?;
    if output.status.success() || allow_fail {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", stderr.trim());
    }
}

pub(crate) async fn run_remote_shell_status(cfg: &RemoteConfig, script: &str) -> Result<bool> {
    let output = capture_ssh_bash_output(cfg, script, "running remote shell status").await?;
    Ok(output.status.success())
}

pub(crate) async fn run_remote_shell_capture(
    cfg: &RemoteConfig,
    script: &str,
) -> Result<Option<String>> {
    let output = capture_ssh_bash_output(cfg, script, "running remote shell capture").await?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    } else {
        Ok(None)
    }
}
