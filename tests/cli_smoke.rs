//! CLI smoke tests for the standalone `autonomy` binary.
//!
//! Exercises the CLI surface via `assert_cmd`-style direct invocation through
//! `std::process::Command`. No network — every subcommand tested here works
//! purely offline.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

fn bin_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("target");
    p.push("debug");
    p.push("autonomy");
    p
}

fn autonomy_ledger_url(root: &Path) -> String {
    let path = root.join("target").join("jeryu").join("autonomy.sqlite");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    jeryu::db::config::sqlite_url(&path)
}

fn ensure_built() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let s = Command::new("cargo")
            .args(["build", "-p", "jeryu", "--bin", "autonomy"])
            .status()
            .expect("cargo build");
        assert!(s.success(), "cargo build --bin autonomy failed");
    });
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        out.status.success(),
        "git {:?} failed\nstdout={}\nstderr={}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn copy_profile_autonomy_fixture(repo: &Path) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".jeryu/autonomy");
    let dst = repo.join(".jeryu/autonomy");
    std::fs::create_dir_all(dst.join("policies")).unwrap();
    std::fs::create_dir_all(dst.join("prompts")).unwrap();
    for name in [
        "risk.yml",
        "approvals.yml",
        "release.yml",
        "protected-paths.yml",
        "freeze.yml",
    ] {
        std::fs::copy(
            src.join("policies").join(name),
            dst.join("policies").join(name),
        )
        .unwrap();
    }
    std::fs::copy(
        src.join("prompts/reviewer-nightwatch.md"),
        dst.join("prompts/reviewer-nightwatch.md"),
    )
    .unwrap();
}

fn init_repo_with_single_commit(repo: &Path) {
    git(repo, &["init", "-b", "main"]);
    git(repo, &["config", "user.email", "ci@example.invalid"]);
    git(repo, &["config", "user.name", "CI Smoke"]);
    std::fs::write(repo.join("README.md"), "initial\n").unwrap();
    git(repo, &["add", "README.md"]);
    git(repo, &["commit", "-m", "initial"]);
}

fn add_recent_merge_commit(repo: &Path) {
    git(repo, &["checkout", "-b", "feature/profile-shadow"]);
    std::fs::write(repo.join("feature.txt"), "feature\n").unwrap();
    git(repo, &["add", "feature.txt"]);
    git(repo, &["commit", "-m", "feature change"]);
    git(repo, &["checkout", "main"]);
    std::fs::write(repo.join("main.txt"), "main\n").unwrap();
    git(repo, &["add", "main.txt"]);
    git(repo, &["commit", "-m", "main change"]);
    git(
        repo,
        &[
            "merge",
            "--no-ff",
            "feature/profile-shadow",
            "-m",
            "Merge profile shadow fixture",
        ],
    );
}

fn run_profile_validate_with_url(repo: &Path, db_url: &str) -> Output {
    let mut last = None;
    for attempt in 0..5 {
        let out = Command::new(bin_path())
            .current_dir(repo)
            .args(["profile", "validate", "--profile", "sovereign_plus"])
            .env("JERYU_DATABASE_URL", db_url)
            .output()
            .expect("profile validate");
        if out.status.success() || !profile_validate_missed_ledger_pool(&out) {
            return out;
        }
        last = Some(out);
        if attempt < 4 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    last.expect("profile validate output")
}

fn profile_validate_missed_ledger_pool(out: &Output) -> bool {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    combined.contains("no ledger pool supplied")
}

fn run_profile_validate(repo: &Path) -> Output {
    let ledger = tempfile::tempdir().expect("temp ledger dir");
    let db_url = autonomy_ledger_url(ledger.path());
    run_profile_validate_with_url(repo, &db_url)
}

fn run_kill_bell_status(db_url: &str) -> Output {
    Command::new(bin_path())
        .args(["kill-bell", "status"])
        .env("JERYU_DATABASE_URL", db_url)
        .output()
        .expect("kill-bell status")
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
            &format!("{}/.jeryu/autonomy", env!("CARGO_MANIFEST_DIR")),
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
    let root = tmp.path().join(".jeryu/autonomy");
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
            &format!("{}/.jeryu/autonomy", env!("CARGO_MANIFEST_DIR")),
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
            &format!("{}/.jeryu/autonomy", env!("CARGO_MANIFEST_DIR")),
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
fn kill_bell_status_accepts_sqlite_autonomy_ledger_url() {
    ensure_built();
    let ledger = tempfile::tempdir().unwrap();
    let db_url = autonomy_ledger_url(ledger.path());

    let out = run_kill_bell_status(&db_url);

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("kill-bell JSON");
    assert_eq!(v["state"], "armed");
}

#[test]
fn kill_bell_status_reuses_sqlite_autonomy_ledger_url() {
    ensure_built();
    let ledger = tempfile::tempdir().unwrap();
    let db_url = autonomy_ledger_url(ledger.path());

    let out = run_kill_bell_status(&db_url);

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("kill-bell JSON");
    assert_eq!(v["state"], "armed");
}

#[test]
fn profile_validate_uses_recent_shadow_report_when_above_threshold() {
    ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let ledger = tempfile::tempdir().unwrap();
    let db_url = autonomy_ledger_url(ledger.path());
    copy_profile_autonomy_fixture(tmp.path());
    init_repo_with_single_commit(tmp.path());
    add_recent_merge_commit(tmp.path());

    let status = run_kill_bell_status(&db_url);
    assert!(
        status.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );

    let out = run_profile_validate_with_url(tmp.path(), &db_url);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("profile validate JSON");
    assert_eq!(v["effective_profile"], "sovereign_plus");
    assert_eq!(v["all_passed"], true);
    assert!(
        v["passed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g.as_str() == Some("shadow_agreement_recent"))
    );
}

#[test]
fn profile_validate_fails_closed_when_shadow_report_missing() {
    ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    copy_profile_autonomy_fixture(tmp.path());

    let out = run_profile_validate(tmp.path());
    assert_eq!(
        out.status.code(),
        Some(78),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("profile validate JSON");
    assert_eq!(v["effective_profile"], "sovereign");
    let failures = v["failed"].as_array().unwrap();
    assert!(failures.iter().any(|f| {
        f["guardrail"] == "shadow_agreement_recent"
            && f["reason"]
                .as_str()
                .unwrap()
                .contains("no recent shadow report found")
    }));
}

#[test]
fn profile_validate_rejects_shadow_report_below_threshold() {
    ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    copy_profile_autonomy_fixture(tmp.path());
    init_repo_with_single_commit(tmp.path());

    let out = run_profile_validate(tmp.path());
    assert_eq!(
        out.status.code(),
        Some(78),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("profile validate JSON");
    assert_eq!(v["effective_profile"], "sovereign");
    let failures = v["failed"].as_array().unwrap();
    assert!(failures.iter().any(|f| {
        f["guardrail"] == "shadow_agreement_recent"
            && f["reason"]
                .as_str()
                .unwrap()
                .contains("latest shadow agreement_rate is 0.0000")
    }));
}

#[test]
fn init_subcommand_fails_on_existing_without_force() {
    ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".jeryu/autonomy")).unwrap();
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
