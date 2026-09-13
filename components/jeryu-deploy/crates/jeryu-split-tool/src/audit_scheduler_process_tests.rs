use super::*;
use std::time::{Duration, Instant};

fn deadline(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new("/usr/bin/timeout");
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args(["--kill-after=1s", "0.2s", program])
        .args(args);
    command
}

#[test]
fn bounded_read_preserves_complete_bytes_and_nonzero_exit() {
    let result = output(&mut deadline("/usr/bin/printf", &["hello"]), 5, 5).unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"hello");
    assert!(result.stderr.is_empty());
    let result = output(&mut deadline("/usr/bin/false", &[]), 5, 5).unwrap();
    assert_eq!(result.status.code(), Some(1));
}

#[test]
fn exhausted_stdout_or_stderr_cannot_be_a_partial_success() {
    assert!(output(&mut deadline("/usr/bin/printf", &["hello"]), 4, 5).is_err());
    let mut command = deadline("/usr/bin/git", &["--not-a-real-git-argument"]);
    assert!(output(&mut command, 1024, 4).is_err());
}

#[test]
fn a_stalled_child_returns_an_explicit_deadline_failure() {
    let started = Instant::now();
    let error = output(&mut deadline("/usr/bin/sleep", &["5"]), 1024, 1024).unwrap_err();
    assert!(format!("{error:#}").contains("deadline exhausted"));
    assert!(started.elapsed() < Duration::from_secs(3));
}
