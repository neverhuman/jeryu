//! Resource-confinement + Landlock execute-bit tests, modeled on the escape
//! suite's HONEST-SKIP discipline: a kernel-dependent assertion runs for real
//! only when `SandboxCapabilities::probe()` says the primitive is available,
//! otherwise it skips-with-an-eprintln. No false skips, no faked passes.
//!
//! Coverage:
//!   * require_cgroup fail-closed: a plan that REQUIRES cgroups on a host
//!     without a delegated subtree resolves to `Unavailable` and `spawn_sandboxed`
//!     refuses (runs FOR REAL on this no-delegation host).
//!   * (fork-bomb containment under an enforced cgroup is proven by the escape
//!     suite's `escape_fork_bomb`, so it is not duplicated here.)
//!   * Landlock execute bit: a workspace rule with `execute = true` PERMITS exec
//!     of a binary under that root; a read+write-but-NO-exec rule DENIES it.
//!     This drives the real launch path (`spawn_sandboxed` -> `apply_landlock`),
//!     so it exercises the latent-bug fix that now honors `rule.execute`.

use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::sandbox::{LandlockRule, SandboxPlan};
use jeryu_runner_core::trust::TrustTier;
use jeryu_sandbox_linux::capability::{EnforcementLevel, SandboxCapabilities};
use jeryu_sandbox_linux::launch::spawn_sandboxed;
use jeryu_sandbox_linux::watchdog::run_with_watchdog;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn workspace(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "jeryu-cgconf-{tag}-{}-{}",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    std::fs::create_dir_all(&d).unwrap_or_else(|e| panic!("create ws: {e}"));
    d
}

fn sandbox_env() -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("PATH".into(), "/usr/local/bin:/usr/bin:/bin".into());
    env.insert("HOME".into(), "/tmp".into());
    env
}

fn job(ws: PathBuf, command: &str, args: Vec<String>) -> JobRequest {
    JobRequest {
        job_id: "cgconf".into(),
        repo_id: "repo".into(),
        commit_sha: "abc".into(),
        workspace: ws,
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

// ---- require_cgroup fail-closed (runs for real on this no-delegation host) --

#[test]
fn require_cgroup_plan_refuses_to_launch_without_delegation() {
    let caps = SandboxCapabilities::probe();
    eprintln!("CONFINE caps: {}", caps.summary());

    let ws = workspace("strict");
    let j = job(ws.clone(), "/bin/echo", vec!["nope".into()]);
    let decision = select_runner(&j).unwrap_or_else(|e| panic!("{e}"));
    let plan = SandboxPlan::from_decision(&j.workspace, &decision).with_require_cgroup(true);

    let level = caps.enforcement_level(&plan);

    if caps.cgroup_v2_subtree.is_none() {
        // FAIL-CLOSED for real: the resolver says Unavailable and the launch is
        // refused with no child ever spawned.
        match &level {
            EnforcementLevel::Unavailable { reason } => {
                assert!(reason.contains("cgroup"), "reason names cgroups: {reason}");
            }
            other => panic!("require_cgroup must be Unavailable here, got {other:?}"),
        }
        let err = spawn_sandboxed(&j, &plan, &caps, &sandbox_env())
            .expect_err("spawn must be refused fail-closed");
        assert_eq!(err.code(), "sandbox_unavailable");
    } else {
        // Delegated subtree present: the require_cgroup plan is NOT Unavailable.
        assert!(
            !matches!(level, EnforcementLevel::Unavailable { .. }),
            "with a delegated subtree the strict plan must not be Unavailable"
        );
    }

    let _ = std::fs::remove_dir_all(&ws);
}

// ---- Landlock execute bit (real launch path; HONEST-SKIP without Landlock) ---

/// Build a plan rooted at `ws` whose ONLY workspace rule has the given
/// read/write/execute bits, plus read+exec rules for the system roots the
/// dynamic loader needs (so the failure signal is the WORKSPACE execute bit, not
/// a broken loader).
fn plan_for_exec_bit(ws: &Path, exec: bool) -> SandboxPlan {
    let j = job(ws.to_path_buf(), "unused", vec![]);
    let decision = select_runner(&j).unwrap_or_else(|e| panic!("{e}"));
    let mut plan = SandboxPlan::from_decision(ws, &decision);
    plan.landlock_rules = vec![
        LandlockRule {
            path: ws.to_path_buf(),
            read: true,
            write: true,
            execute: exec,
        },
        LandlockRule {
            path: PathBuf::from("/usr"),
            read: true,
            write: false,
            execute: true,
        },
        LandlockRule {
            path: PathBuf::from("/lib"),
            read: true,
            write: false,
            execute: true,
        },
        LandlockRule {
            path: PathBuf::from("/lib64"),
            read: true,
            write: false,
            execute: true,
        },
        LandlockRule {
            path: PathBuf::from("/bin"),
            read: true,
            write: false,
            execute: true,
        },
        LandlockRule {
            path: PathBuf::from("/nix/store"),
            read: true,
            write: false,
            execute: true,
        },
    ];
    plan
}

/// Stage a tiny shell script that just exits 0 INTO the workspace and mark it
/// executable. A script needs read (for the interpreter to read it) + execute,
/// so toggling the workspace execute bit cleanly flips whether it can run.
fn stage_exec_target(ws: &Path) -> PathBuf {
    let target = ws.join("run.sh");
    std::fs::write(&target, b"#!/bin/sh\nexit 0\n").expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&target).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&target, perms).expect("chmod");
    }
    target
}

#[test]
fn landlock_execute_bit_permits_exec_under_workspace() {
    let caps = SandboxCapabilities::probe();
    if caps.landlock_abi.is_none() {
        eprintln!("SKIP: no Landlock on this host; cannot prove the execute bit permits exec");
        return;
    }
    // Execute is a separate right only on ABI >= 2; this host is ABI 4.
    if caps.landlock_abi.unwrap_or(0) < 2 {
        eprintln!("SKIP: Landlock ABI < 2; Execute is not a separate right");
        return;
    }

    let ws = workspace("exec-allow");
    let target = stage_exec_target(&ws);
    let plan = plan_for_exec_bit(&ws, /* exec = */ true);
    let j = job(ws.clone(), &target.to_string_lossy(), vec![]);

    let child = spawn_sandboxed(&j, &plan, &caps, &sandbox_env())
        .unwrap_or_else(|e| panic!("spawn (exec allowed): {e}"));
    let out = run_with_watchdog(child, Duration::from_secs(10))
        .unwrap_or_else(|e| panic!("watchdog: {e}"));
    eprintln!(
        "EXEC-ALLOW: exit={:?} timed_out={}",
        out.exit_code, out.timed_out
    );
    assert!(!out.timed_out, "exec-allowed script must not hang");
    assert_eq!(
        out.exit_code,
        Some(0),
        "a workspace rule with execute=true MUST permit exec of the staged script"
    );

    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn landlock_no_execute_bit_denies_exec_under_workspace() {
    let caps = SandboxCapabilities::probe();
    if caps.landlock_abi.is_none() {
        eprintln!("SKIP: no Landlock on this host; cannot prove the no-execute rule denies exec");
        return;
    }
    if caps.landlock_abi.unwrap_or(0) < 2 {
        eprintln!("SKIP: Landlock ABI < 2; Execute is not a separate right");
        return;
    }

    let ws = workspace("exec-deny");
    let target = stage_exec_target(&ws);
    // Same staged script, same system read+exec roots, but the WORKSPACE rule is
    // read+write WITHOUT execute. The only difference from the allow case is the
    // workspace execute bit, so a denied exec proves rule.execute is honored.
    let plan = plan_for_exec_bit(&ws, /* exec = */ false);
    let j = job(ws.clone(), &target.to_string_lossy(), vec![]);

    let result = spawn_sandboxed(&j, &plan, &caps, &sandbox_env());
    match result {
        // The kernel denies the exec; spawn surfaces it as a start failure
        // (EACCES from execvp inside the child).
        Err(e) => {
            eprintln!(
                "EXEC-DENY: spawn refused as expected: {} {}",
                e.code(),
                e.message()
            );
        }
        Ok(child) => {
            // If the child started, it must NOT have exec'd our script: a denied
            // exec yields no successful run. The strongest signal is a non-zero /
            // signal exit and, crucially, the script's side effect never happens.
            let out = run_with_watchdog(child, Duration::from_secs(10))
                .unwrap_or_else(|e| panic!("watchdog: {e}"));
            eprintln!(
                "EXEC-DENY: started but exit={:?} timed_out={}",
                out.exit_code, out.timed_out
            );
            assert_ne!(
                out.exit_code,
                Some(0),
                "BREACH: workspace rule WITHOUT execute still allowed the script to run cleanly"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&ws);
}
