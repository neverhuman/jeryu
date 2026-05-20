#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn pre_push_hook() -> PathBuf {
    repo_root().join("ops/git-hooks/pre-push")
}

fn quality_gate_stub(log_path: &Path) -> PathBuf {
    let temp = log_path.parent().expect("log parent");
    let stub = temp.join("quality-gates-stub.sh");
    fs::write(
        &stub,
        format!(
            "#!/usr/bin/env bash\nprintf 'quality-gates\\n' >> '{}'\n",
            log_path.display()
        ),
    )
    .expect("write stub");
    let mut perms = fs::metadata(&stub).expect("stub metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&stub, perms).expect("stub perms");
    stub
}

fn run_pre_push(stdin_payload: &str, quality_gate_script: &Path) -> Output {
    let mut child = Command::new("bash")
        .arg(pre_push_hook())
        .arg("origin")
        .arg("https://example.invalid/repo.git")
        .env("JERYU_PRE_PUSH_QUALITY_GATES", quality_gate_script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pre-push hook");
    let mut stdin = child.stdin.take().expect("hook stdin");
    stdin
        .write_all(stdin_payload.as_bytes())
        .expect("write hook stdin");
    drop(stdin);
    child.wait_with_output().expect("wait for hook")
}

#[test]
fn pre_push_rejects_updates_to_main_before_quality_gates() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("quality-gates.log");
    let stub = quality_gate_stub(&log_path);

    let output = run_pre_push(
        &format!(
            "refs/heads/main {} refs/heads/main {}\n",
            "a".repeat(40),
            "b".repeat(40)
        ),
        &stub,
    );

    assert!(!output.status.success(), "main pushes should be rejected");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("direct pushes to main are forbidden"),
        "stderr did not explain the refusal: {stderr}"
    );
    assert!(
        !log_path.exists(),
        "quality gate should not run when main is targeted"
    );
}

#[test]
fn pre_push_delegates_non_main_pushes_to_quality_gates() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("quality-gates.log");
    let stub = quality_gate_stub(&log_path);

    let output = run_pre_push(
        &format!(
            "refs/heads/feature {} refs/heads/feature {}\n",
            "a".repeat(40),
            "b".repeat(40)
        ),
        &stub,
    );

    assert!(
        output.status.success(),
        "non-main pushes should still reach the quality gate"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.is_empty(),
        "hook should stay quiet on stdout: {stdout}"
    );
    assert!(
        stderr.is_empty(),
        "hook should stay quiet on stderr for non-main pushes: {stderr}"
    );
    let log = fs::read_to_string(&log_path).expect("quality gate log");
    assert!(
        log.contains("quality-gates"),
        "quality gate stub was not invoked"
    );
}
