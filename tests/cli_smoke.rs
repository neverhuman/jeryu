//! CLI smoke tests for the standalone `autonomy` binary.
//!
//! Exercises the CLI surface via `assert_cmd`-style direct invocation through
//! `std::process::Command`. No network — every subcommand tested here works
//! purely offline.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn bin_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("target");
    p.push("debug");
    p.push("autonomy");
    p
}

fn ensure_built() {
    if !bin_path().exists() {
        let s = Command::new("cargo")
            .args(["build", "-p", "jeryu", "--bin", "autonomy"])
            .status()
            .expect("cargo build");
        assert!(s.success(), "cargo build --bin autonomy failed");
    }
}

#[test]
fn help_subcommand_lists_all_commands() {
    ensure_built();
    let out = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("run --help");
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    for needed in &["doctor", "review", "judge", "evidence", "init"] {
        assert!(
            s.contains(needed),
            "missing subcommand '{}' in --help output:\n{}",
            needed,
            s
        );
    }
}

#[test]
fn evidence_subcommand_emits_signed_pack() {
    ensure_built();
    let out = Command::new(bin_path())
        .args([
            "evidence",
            "--repo",
            "org/proj",
            "--head-sha",
            &"a".repeat(40),
            "--base-sha",
            &"b".repeat(40),
            "--policy-sha",
            &"c".repeat(40),
            "--risk",
            "R2",
            "--files",
            "src/foo.rs:10:5,src/bar.rs:3:1:auth;crypto",
            "--sign",
        ])
        .output()
        .expect("run evidence");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON pack");
    assert_eq!(v["schema"], "vibegate.evidence_pack.v1");
    assert_eq!(v["risk"], "R2");
    assert_eq!(v["changed_files"].as_array().unwrap().len(), 2);
    assert!(v["signature"].is_object(), "should be signed");
    assert_eq!(
        v["signature"]["algo"], "ed25519",
        "signature must use real ed25519"
    );
}

#[test]
fn evidence_subcommand_rejects_bad_files_arg() {
    ensure_built();
    let out = Command::new(bin_path())
        .args([
            "evidence",
            "--head-sha",
            &"a".repeat(40),
            "--base-sha",
            &"b".repeat(40),
            "--policy-sha",
            &"c".repeat(40),
            "--files",
            "bad_format_no_colons",
        ])
        .output()
        .expect("run evidence");
    assert!(!out.status.success(), "should fail on bad --files");
}

#[test]
fn judge_subcommand_reads_stdin_receipts() {
    ensure_built();
    // First build an Evidence Pack.
    let pack_out = Command::new(bin_path())
        .args([
            "evidence",
            "--repo",
            "org/proj",
            "--head-sha",
            &"a".repeat(40),
            "--base-sha",
            &"b".repeat(40),
            "--policy-sha",
            &"c".repeat(40),
            "--risk",
            "R2",
            "--sign",
        ])
        .output()
        .expect("evidence");
    assert!(pack_out.status.success());
    let pack_path =
        std::env::temp_dir().join(format!("jeryu-judge-pack-{}.json", std::process::id()));
    std::fs::write(&pack_path, &pack_out.stdout).unwrap();

    // Stdin receipts: empty array.
    let mut child = Command::new(bin_path())
        .args([
            "judge",
            "--pack",
            pack_path.to_str().unwrap(),
            "--receipts",
            "-",
            "--autonomy-dir",
            &format!("{}/.autonomy", env!("CARGO_MANIFEST_DIR")),
            "--repo",
            "org/proj",
            "--target-branch",
            "main",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("judge spawn");
    child.stdin.as_mut().unwrap().write_all(b"[]").unwrap();
    let out = child.wait_with_output().expect("judge wait");
    // R2 with 0 receipts → quorum insufficient → RequireHuman → exit 78.
    assert_eq!(
        out.status.code(),
        Some(78),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON verdict");
    assert_eq!(v["schema"], "vibegate.gate_verdict.v1");
    assert_eq!(v["decision"], "require_human");
    std::fs::remove_file(&pack_path).ok();
}

#[test]
fn init_subcommand_scaffolds_minimal_layout() {
    ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(bin_path())
        .args(["init", "--repo-root", tmp.path().to_str().unwrap()])
        .output()
        .expect("run init");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let root = tmp.path().join(".autonomy");
    assert!(root.exists());
    assert!(root.join("autonomy.yml").exists());
    for sub in &[
        "agents",
        "policies",
        "providers",
        "prompts",
        "schemas",
        "keys",
        "flags",
    ] {
        assert!(root.join(sub).exists(), "missing subdir {}", sub);
    }
}

#[test]
fn shadow_subcommand_emits_summary() {
    ensure_built();
    let out = Command::new(bin_path())
        .args([
            "shadow",
            "--repo-root",
            env!("CARGO_MANIFEST_DIR"),
            "--autonomy-dir",
            &format!("{}/.autonomy", env!("CARGO_MANIFEST_DIR")),
            "--max-commits",
            "3",
            "--since-seconds",
            "86400",
        ])
        .output()
        .expect("run shadow");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("autonomy shadow"), "missing header in: {s}");
    assert!(s.contains("commits analyzed"), "missing count line in: {s}");
}

#[test]
fn shadow_subcommand_emits_json_when_requested() {
    ensure_built();
    let out = Command::new(bin_path())
        .args([
            "shadow",
            "--repo-root",
            env!("CARGO_MANIFEST_DIR"),
            "--autonomy-dir",
            &format!("{}/.autonomy", env!("CARGO_MANIFEST_DIR")),
            "--max-commits",
            "3",
            "--since-seconds",
            "86400",
            "--json",
        ])
        .output()
        .expect("run shadow --json");
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
    assert!(v["summary"].is_object());
    // The shadow CLI emits per-commit results under the `results` key (see
    // `src/autonomy/shadow.rs::render_json`). Earlier prototypes used
    // `entries`; the canonical key is `results`.
    assert!(v["results"].is_array());
    assert!(v["summary"]["by_tier"]["R2"].is_number());
}

#[test]
fn init_subcommand_fails_on_existing_without_force() {
    ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join(".autonomy")).unwrap();
    let out = Command::new(bin_path())
        .args(["init", "--repo-root", tmp.path().to_str().unwrap()])
        .output()
        .expect("init");
    assert!(
        !out.status.success(),
        "init should refuse existing dir without --force"
    );
    // With --force it succeeds.
    let out2 = Command::new(bin_path())
        .args([
            "init",
            "--repo-root",
            tmp.path().to_str().unwrap(),
            "--force",
        ])
        .output()
        .expect("init force");
    assert!(out2.status.success(), "init --force should succeed");
}
