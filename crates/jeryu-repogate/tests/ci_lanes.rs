use std::fs;
use std::path::Path;

use jeryu_repogate::{CI_LANES_RELATIVE_PATH, run_ci_lanes_check, run_ci_lanes_list};

fn write_file(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn write_minimal_repo(root: &Path, workflow_extra: &str, lane_command: &str) {
    write_file(
        root,
        CI_LANES_RELATIVE_PATH,
        r#"
schema_version = 1
worker_count = 40
jankurai_version = "jankurai 1.6.10"
allowed_setup_commands = [
  '''git init .
git remote add origin "$GITHUB_SERVER_URL/$GITHUB_REPOSITORY"
git fetch --depth 1 origin "$GITHUB_SHA"
git checkout --detach FETCH_HEAD''',
  "rustup toolchain install 1.95.0 --profile minimal",
]

[[lanes]]
id = "ci-fast"
workflow = ".github/workflows/ci-fast.yml"
job = "affected-fast"
command = "bash ops/ci/ci-fast.sh"
full = true
"#,
    );
    write_file(
        root,
        ".github/workflows/ci-fast.yml",
        &format!(
            r#"name: ci-fast
jobs:
  affected-fast:
    steps:
      - name: Checkout
        run: |
          git init .
          git remote add origin "$GITHUB_SERVER_URL/$GITHUB_REPOSITORY"
          git fetch --depth 1 origin "$GITHUB_SHA"
          git checkout --detach FETCH_HEAD
      - name: Toolchain
        run: rustup toolchain install 1.95.0 --profile minimal
      - name: Lane
        run: {lane_command}
{workflow_extra}"#
        ),
    );
}

#[test]
fn ci_lanes_check_accepts_thin_declared_workflow() {
    let dir = tempfile::tempdir().unwrap();
    write_minimal_repo(dir.path(), "", "bash ops/ci/ci-fast.sh");
    let outcome = run_ci_lanes_check(dir.path()).unwrap();
    assert_eq!(outcome.exit_code, 0, "{:?}", outcome.stdout);
    assert!(outcome.stdout[0].contains("ci-lanes check ok"));
}

#[test]
fn ci_lanes_check_rejects_undeclared_workflow_run() {
    let dir = tempfile::tempdir().unwrap();
    write_minimal_repo(
        dir.path(),
        "      - name: Hosted-only drift\n        run: cargo test --workspace\n",
        "bash ops/ci/ci-fast.sh",
    );
    let outcome = run_ci_lanes_check(dir.path()).unwrap();
    assert_eq!(outcome.exit_code, 1);
    assert!(
        outcome
            .stdout
            .iter()
            .any(|line| line.contains("undeclared run command `cargo test --workspace`"))
    );
}

#[test]
fn ci_lanes_check_rejects_missing_lane_command() {
    let dir = tempfile::tempdir().unwrap();
    write_minimal_repo(dir.path(), "", "bash ops/ci/not-the-lane.sh");
    let outcome = run_ci_lanes_check(dir.path()).unwrap();
    assert_eq!(outcome.exit_code, 1);
    assert!(
        outcome
            .stdout
            .iter()
            .any(|line| line.contains("missing lane ci-fast command"))
    );
}

#[test]
fn ci_lanes_check_rejects_missing_manifest_job() {
    let dir = tempfile::tempdir().unwrap();
    write_minimal_repo(dir.path(), "", "bash ops/ci/ci-fast.sh");
    write_file(
        dir.path(),
        ".github/workflows/ci-fast.yml",
        r#"name: ci-fast
jobs:
  renamed-fast:
    steps:
      - name: Lane
        run: bash ops/ci/ci-fast.sh
"#,
    );

    let outcome = run_ci_lanes_check(dir.path()).unwrap();
    assert_eq!(outcome.exit_code, 1);
    assert!(outcome.stdout.iter().any(|line| {
        line.contains(
            "workflow .github/workflows/ci-fast.yml does not declare manifest job affected-fast",
        )
    }));
}

#[test]
fn ci_lanes_list_emits_full_lane_commands() {
    let dir = tempfile::tempdir().unwrap();
    write_minimal_repo(dir.path(), "", "bash ops/ci/ci-fast.sh");
    let outcome = run_ci_lanes_list(dir.path(), true, false).unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(outcome.stdout, vec!["ci-fast\tbash ops/ci/ci-fast.sh"]);
}
