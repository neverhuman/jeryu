//! PTY launch path: `spawn_sandboxed_with_io(ChildIo::Pty)` wires the jailed
//! child's stdin/stdout/stderr to a PTY whose master the supervisor reads. The
//! kernel confinement (Landlock/seccomp/no_new_privs) is applied identically to
//! the piped path; these tests only assert the terminal wiring.

use std::collections::BTreeMap;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::TrustTier;
use jeryu_sandbox_linux::capability::{EnforcementLevel, SandboxCapabilities};
use jeryu_sandbox_linux::launch::{ChildIo, open_pty, spawn_sandboxed_with_io};

fn job(workspace: std::path::PathBuf, command: &str, args: Vec<String>) -> JobRequest {
    JobRequest {
        job_id: "pty".into(),
        repo_id: "repo".into(),
        commit_sha: "abc".into(),
        workspace,
        command: command.into(),
        args,
        env: BTreeMap::new(),
        trust_tier: TrustTier::T1ProtectedInternal,
        requested_runner: None,
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::Default,
        token_policy: TokenPolicy::ReadOnly,
        timeout_ms: 10_000,
        fork: false,
    }
}

fn sandbox_env() -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("PATH".into(), "/usr/local/bin:/usr/bin:/bin".into());
    env.insert("HOME".into(), "/tmp".into());
    env
}

/// Drain the PTY master until `needle` appears, the child closes the slave (EIO),
/// or the deadline elapses. Returns whatever was read.
fn read_master_until(master: std::os::fd::OwnedFd, needle: &str) -> String {
    let mut master = std::fs::File::from(master);
    let mut buf = Vec::new();
    let mut chunk = [0u8; 256];
    let start = Instant::now();
    loop {
        match master.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if String::from_utf8_lossy(&buf).contains(needle) {
                    break;
                }
            }
            // Linux returns EIO on the master once every slave fd has closed.
            Err(_) => break,
        }
        if start.elapsed() > Duration::from_secs(10) {
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn skip_if_unavailable(plan: &SandboxPlan, caps: &SandboxCapabilities, ws: &std::path::Path) -> bool {
    if let EnforcementLevel::Unavailable { reason } = caps.enforcement_level(plan) {
        eprintln!("SKIP pty: sandbox unavailable: {reason}");
        let _ = std::fs::remove_dir_all(ws);
        return true;
    }
    false
}

#[test]
fn pty_child_output_reaches_master() {
    let caps = SandboxCapabilities::probe();
    let ws = std::env::temp_dir().join(format!("jeryu-pty-out-{}", std::process::id()));
    std::fs::create_dir_all(&ws).expect("workspace");

    let j = job(ws.clone(), "/bin/sh", vec!["-c".into(), "printf 'PTY_OK\\n'".into()]);
    let decision = select_runner(&j).expect("policy");
    // cgroup-relaxed (the host lacks delegation); the jail's Landlock/seccomp
    // still apply. This proves PTY wiring, not cgroup enforcement.
    let plan = SandboxPlan::from_decision(&j.workspace, &decision).with_require_cgroup(false);
    if skip_if_unavailable(&plan, &caps, &ws) {
        return;
    }

    let (master, slave) = open_pty().expect("open_pty");
    let mut child = spawn_sandboxed_with_io(
        &j,
        &plan,
        &caps,
        &sandbox_env(),
        ChildIo::Pty {
            slave_fd: slave.as_raw_fd(),
        },
    )
    .expect("spawn_sandboxed_with_io pty");
    // Parent closes its slave copy so the master observes EOF when the child exits.
    drop(slave);

    let out = read_master_until(master, "PTY_OK");
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&ws);

    assert!(
        out.contains("PTY_OK"),
        "PTY master must receive the jailed child's terminal output, got: {out:?}"
    );
}

#[test]
fn pty_child_sees_stdout_as_a_terminal() {
    let caps = SandboxCapabilities::probe();
    let ws = std::env::temp_dir().join(format!("jeryu-pty-istty-{}", std::process::id()));
    std::fs::create_dir_all(&ws).expect("workspace");

    // `test -t 1` is true ONLY when stdout is a tty — proves the slave is a real
    // PTY, not a pipe. Under ChildIo::Piped this would print NOT_TTY.
    let j = job(
        ws.clone(),
        "/bin/sh",
        vec!["-c".into(), "test -t 1 && printf IS_TTY || printf NOT_TTY".into()],
    );
    let decision = select_runner(&j).expect("policy");
    let plan = SandboxPlan::from_decision(&j.workspace, &decision).with_require_cgroup(false);
    if skip_if_unavailable(&plan, &caps, &ws) {
        return;
    }

    let (master, slave) = open_pty().expect("open_pty");
    let mut child = spawn_sandboxed_with_io(
        &j,
        &plan,
        &caps,
        &sandbox_env(),
        ChildIo::Pty {
            slave_fd: slave.as_raw_fd(),
        },
    )
    .expect("spawn pty");
    drop(slave);

    let out = read_master_until(master, "TTY");
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&ws);

    assert!(
        out.contains("IS_TTY") && !out.contains("NOT_TTY"),
        "jailed child stdout must be a controlling PTY, got: {out:?}"
    );
}
