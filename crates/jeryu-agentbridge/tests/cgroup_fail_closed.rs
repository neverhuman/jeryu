//! Fail-closed proof for the require_cgroup agent-job posture.
//!
//! An [`AgentDriver`] defaults to `require_cgroup = true`: an LLM-driven code
//! generator must never run without ENFORCED cgroup-v2 memory/pids limits. On a
//! host WITHOUT a delegated cgroup-v2 subtree the sandbox resolves to
//! `Unavailable` and `spawn_sandboxed` refuses to launch, surfaced to the driver
//! as `DriverError::SandboxUnavailable`.
//!
//! This host has no cgroup delegation (its leaf is a read-only `session.scope`),
//! so the fail-closed branch runs FOR REAL here — it is NOT a skip. On a host
//! that DOES have delegation we assert the converse (the run is permitted).

use jeryu_agentbridge::driver::{AgentDriver, CommandSpec, DriverError, NullSink, stage_editbot};
use jeryu_sandbox_linux::capability::SandboxCapabilities;
use std::path::PathBuf;
use std::time::Duration;

fn editbot_src() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_jeryu-editbot"))
}

fn cell(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "jeryu-cgfc-{tag}-{}-{}",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn require_cgroup_driver_fails_closed_without_delegated_subtree() {
    let caps = SandboxCapabilities::probe();
    eprintln!("CGROUP-FAIL-CLOSED caps: {}", caps.summary());

    let ws = cell("strict");
    let bot = stage_editbot(&ws, &editbot_src()).expect("stage editbot");
    let spec = CommandSpec::new(bot.to_string_lossy().to_string())
        .env("EDITBOT_TARGET", "EDIT.txt")
        .env("EDITBOT_CONTENT", "should-not-run-without-cgroups");

    // Default driver: require_cgroup = true.
    let driver = AgentDriver::default().with_timeout(Duration::from_secs(10));
    assert!(
        driver.require_cgroup(),
        "the default agent driver must require cgroups"
    );

    let result = driver.run(&ws, &spec, &NullSink);

    if caps.cgroup_v2_subtree.is_none() {
        // FAIL-CLOSED (real on this host): the driver must refuse, and the bot's
        // file must NEVER appear because the bot never ran.
        match result {
            Err(DriverError::SandboxUnavailable(reason)) => {
                eprintln!("CGROUP-FAIL-CLOSED refused as expected: {reason}");
                assert!(
                    reason.contains("cgroup"),
                    "the refusal must name cgroups, got {reason:?}"
                );
            }
            Err(other) => panic!("expected SandboxUnavailable, got {other}"),
            Ok(r) => panic!(
                "BREACH: require_cgroup job ran WITHOUT a delegated subtree (level={})",
                r.enforcement_level
            ),
        }
        assert!(
            !ws.join("EDIT.txt").exists(),
            "BREACH: the bot ran and wrote despite the fail-closed gate"
        );
    } else {
        // Host HAS delegation: the strict job is permitted to launch. We don't
        // assert success (other primitives may degrade), only that it was NOT
        // refused for lack of cgroups.
        if let Err(DriverError::SandboxUnavailable(reason)) = &result {
            assert!(
                !reason.contains("cgroup"),
                "with a delegated subtree the run must NOT be refused for cgroups: {reason}"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn opt_out_driver_runs_on_this_no_delegation_host() {
    // Proves the gate is OPT-IN: with require_cgroup=false the SAME job runs (or
    // honestly skips on a host that lacks the Landlock/seccomp/nnp floor), it is
    // NOT refused for cgroups. This is what keeps the #57 jail tests alive here.
    let ws = cell("optout");
    let bot = stage_editbot(&ws, &editbot_src()).expect("stage editbot");
    let spec = CommandSpec::new(bot.to_string_lossy().to_string())
        .env("EDITBOT_TARGET", "EDIT.txt")
        .env("EDITBOT_CONTENT", "ran-without-cgroup-requirement");

    let driver = AgentDriver::default()
        .with_require_cgroup(false)
        .with_timeout(Duration::from_secs(10));

    match driver.run(&ws, &spec, &NullSink) {
        Ok(r) => {
            eprintln!(
                "OPT-OUT ran: level={} exit={:?}",
                r.enforcement_level, r.exit_code
            );
            // It must not have been refused for cgroups; on this host it should
            // actually succeed under the Landlock/seccomp jail.
            assert!(
                r.enforcement_level != "unavailable",
                "opt-out job must not resolve Unavailable on this host"
            );
        }
        Err(DriverError::SandboxUnavailable(reason)) => {
            // Only legal if the no_new_privs floor is genuinely missing — never
            // for cgroups, since we opted out.
            assert!(
                !reason.contains("cgroup"),
                "opt-out run must never be refused for cgroups: {reason}"
            );
            eprintln!("OPT-OUT honestly skipped (non-cgroup floor missing): {reason}");
        }
        Err(other) => panic!("unexpected driver error: {other}"),
    }

    let _ = std::fs::remove_dir_all(&ws);
}
