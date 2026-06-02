//! Integration tests for the in-cell agent driver.
//!
//! These prove the driver loop end-to-end with the DETERMINISTIC `jeryu-editbot`
//! (the LLM-CLI placeholder), JAILED inside a cell checkout:
//!
//!   1. the edit-bot runs jailed and its file lands INSIDE the cell;
//!   2. a bot writing OUTSIDE the cell is DENIED by Landlock (host FS untouched)
//!      — honestly SKIPPED when this host has no Landlock, never faked;
//!   3. a runaway is killed by the watchdog timeout (`timed_out == true`);
//!   4. exceeding the output budget kills the child (`budget_exceeded == true`).
//!
//! Following the escape-suite discipline, kernel-dependent assertions are
//! skipped-with-an-eprintln ONLY when `SandboxCapabilities::probe()` says the
//! relevant primitive is genuinely absent.

use jeryu_agentbridge::driver::{
    AgentDriver, AgentEvent, AgentRunResult, CollectingSink, CommandSpec, DriverError,
    stage_editbot,
};
use jeryu_sandbox_linux::capability::SandboxCapabilities;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Path to the compiled deterministic edit-bot, injected by Cargo for the bin
/// target of this crate.
fn editbot_src() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_jeryu-editbot"))
}

/// Make a unique throwaway cell workspace directory.
fn cell_workspace(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jeryu-agentcell-{tag}-{}-{}",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create cell {}: {e}", dir.display()));
    dir
}

/// Run the driver, mapping a fail-closed `SandboxUnavailable` into an honest
/// skip (returns `None`) instead of a test failure, exactly like the escape
/// suite skips when a primitive is missing.
fn run_or_skip(
    driver: &AgentDriver,
    workspace: &Path,
    spec: &CommandSpec,
    sink: &CollectingSink,
) -> Option<AgentRunResult> {
    match driver.run(workspace, spec, sink) {
        Ok(result) => Some(result),
        Err(DriverError::SandboxUnavailable(reason)) => {
            eprintln!("SKIP: sandbox unavailable (cannot fail closed): {reason}");
            None
        }
        Err(other) => panic!("driver run failed unexpectedly: {other}"),
    }
}

#[test]
fn editbot_writes_inside_the_cell() {
    let ws = cell_workspace("inside");
    let bot = stage_editbot(&ws, &editbot_src()).expect("stage editbot into cell");
    assert!(bot.starts_with(&ws), "staged bot must live under the cell");

    let driver = AgentDriver::default().with_timeout(Duration::from_secs(10));
    // Relative target resolves against the sandbox cwd, which IS the cell.
    let spec = CommandSpec::new(bot.to_string_lossy().to_string())
        .env("EDITBOT_TARGET", "EDIT.txt")
        .env("EDITBOT_CONTENT", "hello-from-the-cell");

    let sink = CollectingSink::new();
    let Some(result) = run_or_skip(&driver, &ws, &spec, &sink) else {
        let _ = std::fs::remove_dir_all(&ws);
        return;
    };

    assert!(
        result.succeeded(),
        "edit-bot must exit 0 inside the cell (exit={:?} timed_out={} budget_exceeded={}, level={})",
        result.exit_code,
        result.timed_out,
        result.budget_exceeded,
        result.enforcement_level
    );

    let edited = ws.join("EDIT.txt");
    assert!(
        edited.is_file(),
        "EDIT.txt must exist INSIDE the cell at {}",
        edited.display()
    );
    assert_eq!(
        std::fs::read_to_string(&edited).expect("read EDIT.txt"),
        "hello-from-the-cell"
    );

    // The sink saw the lifecycle: Started ... Finished.
    let events = sink.events();
    assert!(matches!(events.first(), Some(AgentEvent::Started { .. })));
    assert!(matches!(events.last(), Some(AgentEvent::Finished { .. })));

    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn editbot_writing_outside_the_cell_is_denied_by_landlock() {
    let caps = SandboxCapabilities::probe();

    let ws = cell_workspace("outside");
    let bot = stage_editbot(&ws, &editbot_src()).expect("stage editbot into cell");

    // A DAC-WRITABLE target OUTSIDE the cell: only Landlock can block this, so a
    // success here would be a true breach (not a DAC accident). We pre-create the
    // parent and assert the file never appears.
    let outside = std::env::temp_dir().join(format!(
        "jeryu-agentcell-escape-{}-{}.txt",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    let _ = std::fs::remove_file(&outside);

    // HONESTY: without Landlock the workspace-only write confinement cannot be
    // enforced, so skip rather than fake a pass.
    if caps.landlock_abi.is_none() {
        eprintln!("SKIP: landlock unavailable on this host; cannot enforce out-of-cell write deny");
        let _ = std::fs::remove_dir_all(&ws);
        return;
    }

    let driver = AgentDriver::default().with_timeout(Duration::from_secs(10));
    let spec = CommandSpec::new(bot.to_string_lossy().to_string())
        .env("EDITBOT_TARGET", outside.to_string_lossy().to_string())
        .env("EDITBOT_CONTENT", "escaped");

    let sink = CollectingSink::new();
    let Some(result) = run_or_skip(&driver, &ws, &spec, &sink) else {
        let _ = std::fs::remove_dir_all(&ws);
        return;
    };

    // The host filesystem MUST be untouched: the out-of-cell file never lands.
    assert!(
        !outside.exists(),
        "Landlock BREACH: edit-bot wrote OUTSIDE the cell at {}",
        outside.display()
    );
    // And the driver surfaces the bot's failure (non-zero exit), not a clean run.
    assert!(
        !result.succeeded(),
        "out-of-cell write must surface as a failure, got exit={:?}",
        result.exit_code
    );
    assert_eq!(
        result.exit_code,
        Some(1),
        "edit-bot reports the denied write with exit 1"
    );

    let _ = std::fs::remove_file(&outside);
    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn watchdog_kills_a_runaway_editbot() {
    let ws = cell_workspace("runaway");
    let bot = stage_editbot(&ws, &editbot_src()).expect("stage editbot into cell");

    // 300ms wall-clock budget; the bot spins forever, so it must be killed.
    let driver = AgentDriver::default()
        .with_timeout(Duration::from_millis(300))
        .with_output_budget(1024 * 1024); // large, so timeout (not budget) wins
    let spec = CommandSpec::new(bot.to_string_lossy().to_string()).env("EDITBOT_SPIN", "1");

    let sink = CollectingSink::new();
    let Some(result) = run_or_skip(&driver, &ws, &spec, &sink) else {
        let _ = std::fs::remove_dir_all(&ws);
        return;
    };

    assert!(
        result.timed_out,
        "a runaway must be killed by the watchdog timeout"
    );
    assert!(!result.budget_exceeded, "timeout, not budget, should fire");
    assert!(
        result.elapsed < Duration::from_secs(5),
        "watchdog must kill long before any natural end ({:?})",
        result.elapsed
    );

    // Finished event reflects the timeout.
    let finished = sink.events().into_iter().rev().find_map(|e| match e {
        AgentEvent::Finished {
            timed_out,
            budget_exceeded,
            ..
        } => Some((timed_out, budget_exceeded)),
        _ => None,
    });
    assert_eq!(finished, Some((true, false)));

    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn output_budget_exceeded_kills_the_child() {
    let ws = cell_workspace("budget");
    let bot = stage_editbot(&ws, &editbot_src()).expect("stage editbot into cell");

    // Tiny 4 KiB budget; the bot is told to spew 4 MiB, so the cap must trip and
    // kill it well before it finishes. Generous timeout so budget (not timeout)
    // is the cause.
    let budget = 4 * 1024;
    let driver = AgentDriver::default()
        .with_timeout(Duration::from_secs(30))
        .with_output_budget(budget);
    let spec = CommandSpec::new(bot.to_string_lossy().to_string()).env("EDITBOT_SPEW", "4096"); // KiB

    let sink = CollectingSink::new();
    let Some(result) = run_or_skip(&driver, &ws, &spec, &sink) else {
        let _ = std::fs::remove_dir_all(&ws);
        return;
    };

    assert!(
        result.budget_exceeded,
        "exceeding the output budget must kill the child"
    );
    assert!(!result.timed_out, "budget, not timeout, should fire");
    // Captured output is bounded: the buffers are truncated to the budget even
    // though the bot tried to emit megabytes.
    assert!(
        result.stdout.len() + result.stderr.len() <= budget,
        "captured buffers must be truncated to the budget (got {} bytes)",
        result.stdout.len() + result.stderr.len()
    );
    assert!(
        result.captured_bytes > budget,
        "the driver must have observed more than the budget before killing"
    );

    // A Budget event must have reported a `used` over the limit.
    let tripped = sink.events().iter().any(|e| match e {
        AgentEvent::Budget { used, limit } => *used > *limit,
        _ => false,
    });
    assert!(tripped, "a Budget event must record the over-limit reading");

    let _ = std::fs::remove_dir_all(&ws);
}
