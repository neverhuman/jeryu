//! Ordinary shell behavior checks only; these never build or run an OCI image.
use std::fs;
use std::process::{Command, Output};

const ENTRYPOINT: &str = include_str!("../../../images/agent-sandbox/bin/entrypoint");
const SMOKE: &str = include_str!("../../../ops/agent-sandbox/smoke.sh");

fn shell(program: &str, body: &str, args: &[&str], home: &std::path::Path) -> Output {
    Command::new(program)
        .args(["-c", body, "agent-image-test"])
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .env("HOME", home)
        .env("JERYU_OCI_RUNTIME", home.join("missing-engine"))
        .output()
        .expect("run shell fixture")
}

#[test]
fn entrypoint_creates_home_and_preserves_command_status() {
    let temp = tempfile::tempdir().expect("private fixture");
    let home = temp.path().join("home");
    let output = shell(
        "/bin/sh",
        ENTRYPOINT,
        &["/bin/sh", "-c", "printf agent-started; exit 23"],
        &home,
    );
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(output.stdout, b"agent-started");
    assert!(home.join(".cache").is_dir());
}

#[test]
fn entrypoint_refuses_home_creation_failure_before_agent_exec() {
    let temp = tempfile::tempdir().expect("private fixture");
    let home = temp.path().join("not-a-directory");
    fs::write(&home, b"regular file").expect("create blocked home");
    let output = shell(
        "/bin/sh",
        ENTRYPOINT,
        &["/bin/sh", "-c", "printf forbidden-agent-start"],
        &home,
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert_eq!(fs::read(&home).expect("read blocked home"), b"regular file");
}

#[test]
fn smoke_refuses_missing_engine_and_unsupported_arguments_before_work() {
    let temp = tempfile::tempdir().expect("private fixture");
    for mode in ["smoke", "full"] {
        let output = shell("/bin/bash", SMOKE, &[mode], temp.path());
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr).expect("stderr text");
        assert!(stderr.contains("FAILED (no container runtime:"));
        assert!(output.stdout.is_empty());
    }
    for args in [&["other"][..], &["smoke", "extra"][..]] {
        let output = shell("/bin/bash", SMOKE, args, temp.path());
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8(output.stderr)
                .expect("stderr text")
                .contains("usage:")
        );
        assert!(output.stdout.is_empty());
    }
    assert_eq!(
        fs::read_dir(temp.path()).expect("fixture entries").count(),
        0
    );
}

#[test]
fn smoke_summary_requires_all_twenty_one_successes() {
    let temp = tempfile::tempdir().expect("private fixture");
    let marker = "echo \"agent-sandbox smoke: $passes passed, $fails failed\"";
    let (_, tail) = SMOKE.split_once(marker).expect("actual summary boundary");
    for (passes, fails, accepted) in [
        (21, 0, true),
        (20, 0, false),
        (22, 0, false),
        (20, 1, false),
        (0, 0, false),
    ] {
        let body = format!(
            "set -euo pipefail\npasses={passes}\nfails={fails}\nmode=smoke\nruntime=never-invoked\n{marker}{tail}"
        );
        let output = shell("/bin/bash", &body, &[], temp.path());
        assert_eq!(
            output.status.success(),
            accepted,
            "{passes} passes / {fails} failures"
        );
        assert_eq!(
            String::from_utf8(output.stdout)
                .expect("stdout text")
                .contains("agent-sandbox smoke: PASSED"),
            accepted
        );
    }
}
