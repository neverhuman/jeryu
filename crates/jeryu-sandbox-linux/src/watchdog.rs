//! Wall-clock watchdog that kills a sandboxed child's entire process group on
//! timeout. The launch path puts the child into its own process group
//! (`setpgid(0, 0)` in `pre_exec`) so a single `kill(-pgid, SIGKILL)` reaps the
//! whole subtree even if the job forked grandchildren.

use std::process::Child;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Outcome of a watchdog-supervised run.
#[derive(Debug)]
pub struct WatchdogOutcome {
    /// Captured stdout (empty when output was not piped).
    pub stdout: Vec<u8>,
    /// Captured stderr (empty when output was not piped).
    pub stderr: Vec<u8>,
    /// Exit code, if the process exited normally.
    pub exit_code: Option<i32>,
    /// True when the wall-clock budget was exceeded and the group was killed.
    pub timed_out: bool,
    /// Wall-clock time actually spent.
    pub elapsed: Duration,
}

/// Run a child to completion under a wall-clock `timeout`.
///
/// On timeout the child's process group (`-pid`, since the child is its own
/// group leader) receives `SIGKILL`, the call still reaps the child, and
/// `timed_out` is set so the caller can map it to `ReceiptStatus::TimedOut`.
///
/// `stdout`/`stderr` are drained on dedicated threads so a chatty job cannot
/// deadlock against a full pipe while we wait.
pub fn run_with_watchdog(mut child: Child, timeout: Duration) -> std::io::Result<WatchdogOutcome> {
    let started = Instant::now();
    let pid = child.id() as i32;

    let stdout_handle = child.stdout.take().map(spawn_drainer);
    let stderr_handle = child.stderr.take().map(spawn_drainer);

    // Poll for completion with a coarse-but-responsive cadence rather than
    // blocking forever, so we can enforce the deadline ourselves.
    let mut timed_out = false;
    let exit_code = loop {
        match child.try_wait()? {
            Some(status) => break status.code(),
            None => {
                if started.elapsed() >= timeout {
                    kill_group(pid);
                    timed_out = true;
                    // Reap the (now killed) child to avoid a zombie.
                    let status = child.wait()?;
                    break status.code();
                }
                thread::sleep(poll_interval(started.elapsed(), timeout));
            }
        }
    };

    let stdout = stdout_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    Ok(WatchdogOutcome {
        stdout,
        stderr,
        exit_code,
        timed_out,
        elapsed: started.elapsed(),
    })
}

fn spawn_drainer<R: std::io::Read + Send + 'static>(mut reader: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut reader, &mut buf);
        buf
    })
}

/// Adapt poll cadence to the deadline: tight near the end, relaxed early, so a
/// long-running build is cheap to supervise but a timeout fires promptly.
fn poll_interval(elapsed: Duration, timeout: Duration) -> Duration {
    let remaining = timeout.saturating_sub(elapsed);
    remaining
        .min(Duration::from_millis(25))
        .max(Duration::from_millis(1))
}

/// Send `SIGKILL` to the child's process group. The child is its own group
/// leader (pgid == pid), so the negative-pid form targets the whole subtree.
fn kill_group(pid: i32) {
    // SAFETY: `kill(2)` with a negative pid and SIGKILL is a pure signal
    // delivery to a process group we created; it has no in-process effects and
    // cannot corrupt memory. Failure (ESRCH) is ignored — the group is gone.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}

/// Convenience for callers that need a channel-based completion signal (kept
/// for symmetry / future async integration; the polling loop above is the
/// primary path).
#[allow(dead_code)]
fn drain_channel<T>(rx: &mpsc::Receiver<T>, deadline: Instant) -> Option<T> {
    let now = Instant::now();
    if now >= deadline {
        return None;
    }
    rx.recv_timeout(deadline - now).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    fn group_leader_command(program: &str) -> Command {
        let mut cmd = Command::new(program);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        // SAFETY: setpgid(0,0) in the child is async-signal-safe and only moves
        // the child into its own process group.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        cmd
    }

    #[test]
    fn fast_command_completes_without_timeout() {
        let child = group_leader_command("/bin/echo")
            .arg("hello")
            .spawn()
            .unwrap_or_else(|err| panic!("spawn echo: {err}"));
        let out = run_with_watchdog(child, Duration::from_secs(5))
            .unwrap_or_else(|err| panic!("watchdog: {err}"));
        assert!(!out.timed_out);
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
    }

    #[test]
    fn slow_command_is_killed_on_timeout() {
        // `sleep 30` must be killed by a 200ms watchdog.
        let child = group_leader_command("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap_or_else(|err| panic!("spawn sleep: {err}"));
        let out = run_with_watchdog(child, Duration::from_millis(200))
            .unwrap_or_else(|err| panic!("watchdog: {err}"));
        assert!(out.timed_out, "expected timeout flag");
        assert!(
            out.elapsed < Duration::from_secs(5),
            "watchdog must kill long before the sleep ends"
        );
    }

    #[test]
    fn fork_subtree_is_reaped_via_group_kill() {
        // A shell that backgrounds a long sleep then sleeps itself; killing the
        // group must terminate both promptly.
        let child = group_leader_command("/bin/sh")
            .arg("-c")
            .arg("sleep 30 & sleep 30")
            .spawn()
            .unwrap_or_else(|err| panic!("spawn sh: {err}"));
        let out = run_with_watchdog(child, Duration::from_millis(200))
            .unwrap_or_else(|err| panic!("watchdog: {err}"));
        assert!(out.timed_out);
        assert!(out.elapsed < Duration::from_secs(5));
    }
}
