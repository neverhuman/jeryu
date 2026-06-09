use super::*;
use jeryu_gitd::refs::GitRef;
use std::fs;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_out(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn init_version_repo(root: &Path) -> (String, String) {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "ci@example.invalid"]);
    git(root, &["config", "user.name", "CI"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/demo\"]\n\n[workspace.package]\nversion = \"4.0.0\"\nedition = \"2024\"\nlicense = \"Apache-2.0\"\nrust-version = \"1.95\"\n",
    );
    write(
        root,
        "crates/demo/Cargo.toml",
        "[package]\nname = \"demo\"\nversion.workspace = true\nedition.workspace = true\nlicense.workspace = true\nrust-version.workspace = true\n\n[lib]\npath = \"src/lib.rs\"\n",
    );
    write(root, "crates/demo/src/lib.rs", "pub fn demo() {}\n");
    write(
        root,
        "CHANGELOG.md",
        "# Changelog\n\n## Unreleased\n\n- seed\n",
    );
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "chore: base"]);
    let base = git_out(root, &["rev-parse", "HEAD"]);

    write(root, "docs/feature.md", "feature\n");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "feat: add dashboard signal"]);
    let head = git_out(root, &["rev-parse", "HEAD"]);
    (base, head)
}

fn clone_bare(work: &Path, bare: &Path) {
    let bare_str = bare.to_string_lossy().to_string();
    git(work, &["clone", "--bare", ".", &bare_str]);
}

fn ref_update(ref_name: &str, old_oid: &str, new_oid: &str) -> RefUpdate {
    RefUpdate {
        ref_name: ref_name.to_owned(),
        old_oid: old_oid.to_owned(),
        new_oid: new_oid.to_owned(),
    }
}

#[test]
fn branch_head_skips_tag_only_release_workflow() {
    let workflow = r#"
name: release
on:
  push:
    tags: ['v*']
  workflow_dispatch:
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - run: bash ops/ci/release.sh
"#;

    assert!(!workflow_runs_for_branch_head(
        workflow,
        "refs/heads/codex/feature"
    ));
}

#[test]
fn branch_head_runs_pull_request_workflow_even_with_main_push_filter() {
    let workflow = r#"
name: web
on:
  push:
    branches:
      - main
  pull_request:
jobs:
  web:
    runs-on: ubuntu-latest
    steps:
      - run: bash ops/ci/web.sh
"#;

    assert!(workflow_runs_for_branch_head(
        workflow,
        "refs/heads/codex/feature"
    ));
}

#[test]
fn branch_push_filter_matches_only_named_branches() {
    let workflow = r#"
name: branch-only
on:
  push:
    branches: [main, release/*]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: true
"#;

    assert!(workflow_runs_for_branch_head(workflow, "refs/heads/main"));
    assert!(workflow_runs_for_branch_head(
        workflow,
        "refs/heads/release/next"
    ));
    assert!(!workflow_runs_for_branch_head(
        workflow,
        "refs/heads/codex/feature"
    ));
}

#[test]
fn inline_on_list_runs_for_branch_push() {
    let workflow = r#"
name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: true
"#;

    assert!(workflow_runs_for_branch_head(
        workflow,
        "refs/heads/codex/feature"
    ));
}

#[test]
fn hosted_toolchain_bootstrap_step_is_skipped_locally() {
    assert!(is_hosted_toolchain_bootstrap(
        "rustup toolchain install 1.95.0 --profile minimal"
    ));
    assert!(!is_hosted_toolchain_bootstrap(
        "rustup toolchain install 1.95.0 --profile minimal\ncargo test"
    ));
    assert!(!is_hosted_toolchain_bootstrap("bash ops/ci/ci-fast.sh"));
}

#[test]
fn ref_updates_track_ref_name_and_old_oid() {
    let before = vec![
        GitRef {
            name: "refs/heads/main".to_owned(),
            oid: "aaa".to_owned(),
        },
        GitRef {
            name: "refs/heads/feature".to_owned(),
            oid: "bbb".to_owned(),
        },
    ];
    let after = vec![
        GitRef {
            name: "refs/heads/main".to_owned(),
            oid: "ccc".to_owned(),
        },
        GitRef {
            name: "refs/heads/feature".to_owned(),
            oid: "bbb".to_owned(),
        },
    ];

    let updates = ref_updates(&before, &after);

    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].ref_name, "refs/heads/main");
    assert_eq!(updates[0].old_oid, "aaa");
    assert_eq!(updates[0].new_oid, "ccc");
}

#[test]
fn main_push_writes_single_skip_version_bump_commit() {
    let work = tempfile::tempdir().unwrap();
    let bare = tempfile::tempdir().unwrap();
    let (base, head) = init_version_repo(work.path());
    clone_bare(work.path(), bare.path());

    maybe_bump_main_version(
        "git",
        bare.path(),
        "jeryu",
        "demo",
        &ref_update("refs/heads/main", &base, &head),
    );

    let main = git_out(bare.path(), &["rev-parse", "refs/heads/main"]);
    assert_ne!(main, head);
    assert_eq!(
        git_out(
            bare.path(),
            &["log", "-1", "--format=%s", "refs/heads/main"]
        ),
        "chore(release): v4.1.0 [skip-version]"
    );
    let manifest = git_out(bare.path(), &["show", "refs/heads/main:Cargo.toml"]);
    assert!(manifest.contains("version = \"4.1.0\""));
    let changelog = git_out(bare.path(), &["show", "refs/heads/main:CHANGELOG.md"]);
    assert!(changelog.contains("## v4.1.0 - "));
}

#[test]
fn skip_version_bump_commit_does_not_recurse() {
    let work = tempfile::tempdir().unwrap();
    let bare = tempfile::tempdir().unwrap();
    let (base, head) = init_version_repo(work.path());
    clone_bare(work.path(), bare.path());

    maybe_bump_main_version(
        "git",
        bare.path(),
        "jeryu",
        "demo",
        &ref_update("refs/heads/main", &base, &head),
    );
    let bump = git_out(bare.path(), &["rev-parse", "refs/heads/main"]);

    maybe_bump_main_version(
        "git",
        bare.path(),
        "jeryu",
        "demo",
        &ref_update("refs/heads/main", &head, &bump),
    );

    assert_eq!(
        git_out(bare.path(), &["rev-parse", "refs/heads/main"]),
        bump
    );
    let subjects = git_out(
        bare.path(),
        &["log", "--format=%s", &format!("{base}..refs/heads/main")],
    );
    assert_eq!(subjects.matches("[skip-version]").count(), 1);
}

#[test]
fn non_main_update_does_not_bump_version() {
    let work = tempfile::tempdir().unwrap();
    let bare = tempfile::tempdir().unwrap();
    let (base, head) = init_version_repo(work.path());
    clone_bare(work.path(), bare.path());

    maybe_bump_main_version(
        "git",
        bare.path(),
        "jeryu",
        "demo",
        &ref_update("refs/heads/feature", &base, &head),
    );

    assert_eq!(
        git_out(bare.path(), &["rev-parse", "refs/heads/main"]),
        head
    );
    let manifest = git_out(bare.path(), &["show", "refs/heads/main:Cargo.toml"]);
    assert!(manifest.contains("version = \"4.0.0\""));
}

#[test]
fn concurrent_main_updates_leave_one_release_commit() {
    let work = tempfile::tempdir().unwrap();
    let bare = tempfile::tempdir().unwrap();
    let (base, head) = init_version_repo(work.path());
    clone_bare(work.path(), bare.path());
    let bare_path = bare.path().to_path_buf();

    std::thread::scope(|scope| {
        for _ in 0..4 {
            let base = base.clone();
            let head = head.clone();
            let bare_path = bare_path.clone();
            scope.spawn(move || {
                maybe_bump_main_version(
                    "git",
                    &bare_path,
                    "jeryu",
                    "demo",
                    &ref_update("refs/heads/main", &base, &head),
                );
            });
        }
    });

    let subjects = git_out(
        bare.path(),
        &["log", "--format=%s", &format!("{base}..refs/heads/main")],
    );
    assert_eq!(subjects.matches("[skip-version]").count(), 1);
    assert!(subjects.contains("feat: add dashboard signal"));
}

#[test]
fn mock_flag_gates_workflow_check_run_seeding() {
    // The production forge (no JERYU_CI_MOCK) must NOT seed GitHub-Actions check-runs
    // — it has no Actions runners, so they only produced all-red noise; host-ci's
    // `jeryu/ci` is the real gate. Only the in-process CI-seeding-flow tests opt in.
    // Pure predicate, so this never mutates the shared process env (which would race
    // the parallel seeded-CI tests that read JERYU_CI_MOCK).
    assert!(
        !mock_flag_set(None),
        "unset -> production posture, no seeding"
    );
    assert!(!mock_flag_set(Some("")));
    assert!(!mock_flag_set(Some("0")));
    assert!(!mock_flag_set(Some("  0  ")));
    assert!(mock_flag_set(Some("1")), "opt-in for tests");
    assert!(mock_flag_set(Some("true")));
}
