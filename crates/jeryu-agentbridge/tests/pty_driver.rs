//! Integration tests for the PTY-backed in-cell agent driver: terminal output
//! streams to the sink, inbound control reaches the agent's stdin, and a
//! Terminate command stops a runaway. cgroup-relaxed (the host may lack
//! delegation); honest-skip when the kernel sandbox is unavailable.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jeryu_agentbridge::driver::{
    AgentEvent, AgentRunResult, CollectingSink, CommandSpec, DriverError,
};
use jeryu_agentbridge::pty_driver::{AgentControl, AgentControlSource, NoControl, PtyAgentDriver};

fn cell_workspace(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jeryu-pty-drv-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create cell workspace");
    dir
}

fn run_or_skip<C: AgentControlSource>(
    driver: &PtyAgentDriver,
    ws: &Path,
    spec: &CommandSpec,
    sink: &CollectingSink,
    control: &C,
) -> Option<AgentRunResult> {
    match driver.run(ws, spec, sink, control) {
        Ok(result) => Some(result),
        Err(DriverError::SandboxUnavailable(reason)) => {
            eprintln!("SKIP pty driver: sandbox unavailable: {reason}");
            None
        }
        Err(other) => panic!("pty driver error: {other}"),
    }
}

#[test]
fn streams_terminal_output_to_the_sink() {
    let ws = cell_workspace("out");
    let driver = PtyAgentDriver::default()
        .with_require_cgroup(false)
        .with_timeout(Duration::from_secs(10));
    let spec = CommandSpec::new("/bin/sh")
        .arg("-c")
        .arg("printf 'HELLO_PTY\\n'");
    let sink = CollectingSink::new();

    let Some(result) = run_or_skip(&driver, &ws, &spec, &sink, &NoControl) else {
        let _ = std::fs::remove_dir_all(&ws);
        return;
    };

    let out = String::from_utf8_lossy(&result.stdout);
    assert!(
        out.contains("HELLO_PTY"),
        "captured terminal output should contain the agent's stdout, got {out:?}"
    );
    let events = sink.events();
    assert!(matches!(events.first(), Some(AgentEvent::Started { .. })));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Stdout(s) if s.contains("HELLO_PTY"))),
        "a Stdout event must carry the agent output"
    );
    assert!(matches!(events.last(), Some(AgentEvent::Finished { .. })));
    assert!(result.succeeded(), "clean run must succeed");
    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn control_input_reaches_the_agent_stdin() {
    let ws = cell_workspace("input");
    let driver = PtyAgentDriver::default()
        .with_require_cgroup(false)
        .with_timeout(Duration::from_secs(10));
    // Read ONE line from stdin (the PTY) and echo it back.
    let spec = CommandSpec::new("/bin/sh")
        .arg("-c")
        .arg("read line; printf 'GOT:%s\\n' \"$line\"");
    let sink = CollectingSink::new();

    let (tx, rx) = mpsc::channel();
    tx.send(AgentControl::SendInput(b"ping\n".to_vec()))
        .expect("queue control");

    let Some(result) = run_or_skip(&driver, &ws, &spec, &sink, &rx) else {
        let _ = std::fs::remove_dir_all(&ws);
        return;
    };

    let out = String::from_utf8_lossy(&result.stdout);
    assert!(
        out.contains("GOT:ping"),
        "control SendInput must be delivered to the agent's stdin (expected GOT:ping), got {out:?}"
    );
    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn terminate_stops_a_runaway_agent() {
    let ws = cell_workspace("term");
    let driver = PtyAgentDriver::default()
        .with_require_cgroup(false)
        .with_timeout(Duration::from_secs(30));
    let spec = CommandSpec::new("/bin/sh")
        .arg("-c")
        .arg("while true; do sleep 0.05; done");
    let sink = CollectingSink::new();

    let (tx, rx) = mpsc::channel();
    tx.send(AgentControl::Terminate).expect("queue terminate");

    let started = Instant::now();
    let Some(result) = run_or_skip(&driver, &ws, &spec, &sink, &rx) else {
        let _ = std::fs::remove_dir_all(&ws);
        return;
    };

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "Terminate must stop the runaway well before the 30s wall-clock timeout"
    );
    assert!(
        !result.timed_out,
        "the run ended by Terminate, not the wall-clock timeout"
    );
    assert!(!result.succeeded(), "a terminated agent is not a success");
    let _ = std::fs::remove_dir_all(&ws);
}
