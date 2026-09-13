//! Bound local Git reads before allowing them in the maintainer worker.
//! Exhaustion is an unresolved source failure, never a shortened successful graph.
use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    process::{Command, Output, Stdio},
    thread,
};

fn read(mut pipe: impl Read, maximum: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.by_ref().take(maximum + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "source Git output exceeded its limit"
    );
    Ok(bytes)
}

fn output(command: &mut Command, maximum: u64, maximum_error: u64) -> Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start bounded source Git command")?;
    let stdout = child.stdout.take().context("missing source stdout pipe")?;
    let stderr = child.stderr.take().context("missing source stderr pipe")?;
    // Both streams drain concurrently. Closing an exhausted stream prevents
    // unbounded allocation; the external deadline still bounds the Git child.
    thread::scope(|scope| {
        let out = scope.spawn(move || read(stdout, maximum));
        let err = scope.spawn(move || read(stderr, maximum_error));
        let status = child.wait().context("wait for bounded source Git")?;
        let stdout = out
            .join()
            .map_err(|_| anyhow::anyhow!("source stdout reader failed"))??;
        let stderr = err
            .join()
            .map_err(|_| anyhow::anyhow!("source stderr reader failed"))??;
        ensure!(
            !matches!(status.code(), Some(124 | 137)),
            "source Git deadline exhausted"
        );
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    })
}

pub(super) fn git_output(git: &mut Command) -> Result<Output> {
    // This accepts only the planner's fixed local Git executable and arguments.
    // Its existing closed environment disables hooks, lazy network fetches,
    // signature helpers, fsmonitor and replacement objects.
    ensure!(
        git.get_program() == "/usr/bin/git",
        "source planner requires fixed Git"
    );
    let mut command = Command::new("/usr/bin/timeout");
    command
        .args(["--kill-after=2s", "30s"])
        .arg(git.get_program())
        .args(git.get_args())
        .env_clear()
        .current_dir(
            git.get_current_dir()
                .context("source Git needs a directory")?,
        );
    for (name, value) in git.get_envs() {
        if let Some(value) = value {
            command.env(name, value);
        }
    }
    output(&mut command, 64 * 1024 * 1024, 64 * 1024)
}

#[cfg(test)]
#[path = "audit_scheduler_process_tests.rs"]
mod tests;
