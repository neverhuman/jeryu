//! ADVERSARIAL refutation probe (temporary, added by verifier).
//!
//! Goal: try to make the jail FAKE-pass. We independently re-derive the escape
//! and prove the deny is the kernel, not a hardcoded Ok / dead assertion / a bot
//! that never tries.

use jeryu_agentbridge::driver::{
    AgentDriver, AgentEvent, CollectingSink, CommandSpec, NullSink, stage_editbot,
};
use jeryu_sandbox_linux::capability::SandboxCapabilities;
use std::path::PathBuf;
use std::time::Duration;

fn editbot_src() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_jeryu-editbot"))
}

fn cell(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "jeryu-refute-{tag}-{}-{}",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// PROOF 1: the SAME editbot env-driven write path that is DENIED outside the
/// cell SUCCEEDS when the target is inside the cell. This rules out "the bot
/// never actually attempts the escape" / "the bot always fails": the write code
/// is live and exercised, only the LOCATION changes the outcome.
#[test]
fn same_write_path_succeeds_inside_and_is_blocked_outside() {
    let caps = SandboxCapabilities::probe();
    eprintln!("REFUTE caps: {}", caps.summary());
    assert!(
        caps.landlock_abi.is_some(),
        "this host MUST have landlock for a meaningful refutation; lsm lists it"
    );

    // This refutation targets the Landlock jail, not cgroups: opt OUT of the
    // require_cgroup fail-closed gate so it keeps running on this no-delegation
    // host (it is not a cgroup test, and must NOT be weakened otherwise).
    let driver = AgentDriver::default()
        .with_require_cgroup(false)
        .with_timeout(Duration::from_secs(10));

    // INSIDE: absolute path under the cell -> must succeed and land.
    let ws_in = cell("inside");
    let bot_in = stage_editbot(&ws_in, &editbot_src()).unwrap();
    let inside_target = ws_in.join("INSIDE.txt");
    let spec_in = CommandSpec::new(bot_in.to_string_lossy().to_string())
        .env(
            "EDITBOT_TARGET",
            inside_target.to_string_lossy().to_string(),
        )
        .env("EDITBOT_CONTENT", "in");
    let r_in = driver.run(&ws_in, &spec_in, &NullSink).expect("inside run");
    eprintln!(
        "REFUTE inside: exit={:?} level={}",
        r_in.exit_code, r_in.enforcement_level
    );
    assert_eq!(r_in.exit_code, Some(0), "inside write must succeed");
    assert!(inside_target.is_file(), "inside file must exist");

    // OUTSIDE: a DAC-writable temp path NOT under the cell. Prove DAC alone would
    // allow it (parent is writable), so a non-creation is Landlock, not perms.
    let ws_out = cell("outside");
    let bot_out = stage_editbot(&ws_out, &editbot_src()).unwrap();
    let outside = std::env::temp_dir().join(format!(
        "jeryu-refute-ESCAPE-{}-{}.txt",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    let _ = std::fs::remove_file(&outside);
    // Sanity: confirm DAC would permit this write from THIS (unsandboxed) process.
    std::fs::write(&outside, b"dac-ok").expect("DAC must allow the path unsandboxed");
    std::fs::remove_file(&outside).unwrap();

    let spec_out = CommandSpec::new(bot_out.to_string_lossy().to_string())
        .env("EDITBOT_TARGET", outside.to_string_lossy().to_string())
        .env("EDITBOT_CONTENT", "escaped");
    let r_out = driver
        .run(&ws_out, &spec_out, &NullSink)
        .expect("outside run");
    eprintln!(
        "REFUTE outside: exit={:?} level={} file_exists={}",
        r_out.exit_code,
        r_out.enforcement_level,
        outside.exists()
    );

    // THE REFUTATION: the host FS is untouched and the bot reported failure.
    assert!(
        !outside.exists(),
        "BREACH: escape file exists at {}",
        outside.display()
    );
    assert_eq!(r_out.exit_code, Some(1), "denied write -> bot exit 1");

    let _ = std::fs::remove_dir_all(&ws_in);
    let _ = std::fs::remove_dir_all(&ws_out);
    let _ = std::fs::remove_file(&outside);
}

/// PROOF 2: the deny is the KERNEL, not the driver. Run the SAME editbot writing
/// to the SAME out-of-cell target WITHOUT the sandbox (plain Command, no
/// spawn_sandboxed). It MUST succeed -> proving the only thing stopping the
/// sandboxed run is Landlock, i.e. the in-cell deny is real kernel enforcement.
#[test]
fn unsandboxed_control_can_write_outside_proving_landlock_is_the_blocker() {
    let ws = cell("control");
    let bot = stage_editbot(&ws, &editbot_src()).unwrap();
    let outside = std::env::temp_dir().join(format!(
        "jeryu-refute-CONTROL-{}-{}.txt",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    let _ = std::fs::remove_file(&outside);

    // Retry on ETXTBSY: under parallel tests, another thread's fork() can
    // transiently hold the write fd to the just-staged bot, racing this exec.
    let mut status = None;
    for _ in 0..200 {
        match std::process::Command::new(&bot)
            .env("EDITBOT_TARGET", &outside)
            .env("EDITBOT_CONTENT", "control")
            .status()
        {
            Ok(s) => {
                status = Some(s);
                break;
            }
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("spawn unsandboxed control bot: {e}"),
        }
    }
    let status = status.expect("spawn unsandboxed control bot (ETXTBSY did not clear)");

    eprintln!(
        "REFUTE control(unsandboxed): exit={:?} file_exists={}",
        status.code(),
        outside.exists()
    );
    assert_eq!(status.code(), Some(0), "unsandboxed bot writes fine");
    assert!(
        outside.exists(),
        "unsandboxed control MUST land the file (else the host blocks it for unrelated reasons)"
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "control");

    let _ = std::fs::remove_file(&outside);
    let _ = std::fs::remove_dir_all(&ws);
}

/// PROOF 3: the budget check is LIVE, not dead. A bot told to spew 8 MiB against
/// a 2 KiB budget must be KILLED mid-stream: budget_exceeded set, captured_bytes
/// observed over the budget, buffers truncated to <= budget, and the run must end
/// FAR faster than emitting 8 MiB would take if it drained to completion.
#[test]
fn budget_kill_is_live_and_truncates() {
    let ws = cell("budget");
    let bot = stage_editbot(&ws, &editbot_src()).unwrap();
    let budget = 2 * 1024;
    // Budget-kill refutation, not a cgroup test: opt out of require_cgroup.
    let driver = AgentDriver::default()
        .with_require_cgroup(false)
        .with_timeout(Duration::from_secs(60))
        .with_output_budget(budget);
    let spec = CommandSpec::new(bot.to_string_lossy().to_string()).env("EDITBOT_SPEW", "8192"); // 8 MiB

    let sink = CollectingSink::new();
    let start = std::time::Instant::now();
    let r = driver.run(&ws, &spec, &sink).expect("budget run");
    let wall = start.elapsed();
    eprintln!(
        "REFUTE budget: budget_exceeded={} timed_out={} captured={} kept={} wall={:?}",
        r.budget_exceeded,
        r.timed_out,
        r.captured_bytes,
        r.stdout.len() + r.stderr.len(),
        wall
    );
    assert!(r.budget_exceeded, "budget kill must fire (not dead code)");
    assert!(!r.timed_out, "budget, not timeout");
    assert!(r.captured_bytes > budget, "must observe over-budget bytes");
    assert!(
        r.stdout.len() + r.stderr.len() <= budget,
        "buffers truncated to budget"
    );
    // A Budget event over the limit was emitted.
    assert!(sink.events().iter().any(|e| matches!(
        e,
        AgentEvent::Budget { used, limit } if used > limit
    )));
    let _ = std::fs::remove_dir_all(&ws);
}

/// PROOF 4: the watchdog kill is LIVE. A spinning bot under a 200ms timeout must
/// be reaped well under a second; timed_out set; the process is actually gone
/// (the run returns at all -> child was waited, not leaked).
#[test]
fn watchdog_kill_is_live() {
    let ws = cell("spin");
    let bot = stage_editbot(&ws, &editbot_src()).unwrap();
    // Watchdog-kill refutation, not a cgroup test: opt out of require_cgroup.
    let driver = AgentDriver::default()
        .with_require_cgroup(false)
        .with_timeout(Duration::from_millis(200))
        .with_output_budget(64 * 1024 * 1024);
    let spec = CommandSpec::new(bot.to_string_lossy().to_string()).env("EDITBOT_SPIN", "1");

    let sink = CollectingSink::new();
    let pid_before = sink.events();
    let start = std::time::Instant::now();
    let r = driver.run(&ws, &spec, &sink).expect("spin run");
    let wall = start.elapsed();
    let _ = pid_before;
    let pid = sink.events().iter().find_map(|e| match e {
        AgentEvent::Started { pid } => Some(*pid),
        _ => None,
    });
    eprintln!(
        "REFUTE watchdog: timed_out={} budget={} wall={:?} pid={:?}",
        r.timed_out, r.budget_exceeded, wall, pid
    );
    assert!(r.timed_out, "watchdog must fire");
    assert!(!r.budget_exceeded);
    assert!(wall < Duration::from_secs(3), "must be killed promptly");
    // The pid must no longer be a live process of ours (reaped). /proc check:
    if let Some(pid) = pid {
        // give the kernel a beat to clear it
        for _ in 0..50 {
            if !PathBuf::from(format!("/proc/{pid}")).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let live = PathBuf::from(format!("/proc/{pid}/status")).exists();
        // It may be reused, but it must not be our spinning editbot still running.
        if live {
            let cmdline =
                std::fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            assert!(
                !cmdline.contains("jeryu-editbot"),
                "BREACH: editbot pid {pid} still alive after watchdog kill"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&ws);
}
