//! End-to-end decide/apply tests against a real git repo in a tempdir.
//!
//! Mirrors `jeryu-repogate`'s git-init test idiom: build a throwaway workspace,
//! seed an `origin/main` ref, layer conventional commits on top, then drive the
//! public `decide`/`apply` API and assert the version bump + CHANGELOG roll. The
//! `cargo semver-checks` lane is not exercised here (the temp workspace has no
//! published baseline), so these tests cover the commit-text + floor=patch +
//! `[skip-version]` decision paths deterministically.

use std::path::Path;
use std::process::Command;

use jeryu_wsversion::{apply, commits_in_range, decide, read_workspace_version};

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .expect("spawn git");
    assert!(status.success(), "git {args:?} failed");
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// Build a minimal but valid workspace with a base commit, and set
/// `refs/remotes/origin/main` to that base so `origin/main..HEAD` is meaningful.
fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "ci@example.invalid"]);
    git(root, &["config", "user.name", "CI"]);
    git(root, &["config", "commit.gpgsign", "false"]);

    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/demo\"]\n\n[workspace.package]\nversion = \"4.0.0\"\nedition = \"2021\"\n",
    );
    write(
        root,
        "crates/demo/Cargo.toml",
        "[package]\nname = \"demo\"\nversion.workspace = true\nedition.workspace = true\n",
    );
    write(root, "crates/demo/src/lib.rs", "pub fn api() {}\n");
    write(
        root,
        "CHANGELOG.md",
        "# Changelog\n\n## Unreleased\n\n- seed\n",
    );

    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "chore: base"]);
    git(root, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
}

/// Make a file change and commit it with the given subject.
fn commit(root: &Path, rel: &str, body: &str, subject: &str) {
    write(root, rel, body);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", subject]);
}

#[test]
fn feat_commit_bumps_minor_and_rolls_changelog() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(
        root,
        "crates/demo/src/feature.rs",
        "pub fn added() {}\n",
        "feat(demo): add a feature",
    );

    let decision = decide(root, "origin/main..HEAD").unwrap();
    assert_eq!(decision.from, "4.0.0");
    assert_eq!(decision.to, "4.1.0");
    assert_eq!(decision.bump, "minor");
    assert!(!decision.skipped);

    let commits = commits_in_range(root, "origin/main..HEAD").unwrap();
    apply(root, &decision, &commits).unwrap();

    assert_eq!(read_workspace_version(root).unwrap().to_string(), "4.1.0");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## v4.1.0 - "));
    assert!(changelog.contains("- **demo**: feat(demo): add a feature"));
    // A fresh Unreleased heading remains for the next cycle.
    assert!(changelog.contains("## Unreleased\n\n## v4.1.0"));
}

#[test]
fn chore_only_floors_to_patch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(root, "docs/notes.md", "notes\n", "docs: add notes");

    let decision = decide(root, "origin/main..HEAD").unwrap();
    assert_eq!(decision.to, "4.0.1");
    assert_eq!(decision.bump, "patch");
}

#[test]
fn breaking_bang_bumps_major() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(
        root,
        "crates/demo/src/lib.rs",
        "pub fn api2() {}\n",
        "feat(demo)!: remove the old api",
    );

    let decision = decide(root, "origin/main..HEAD").unwrap();
    assert_eq!(decision.to, "5.0.0");
    assert_eq!(decision.bump, "major");
}

#[test]
fn breaking_change_footer_bumps_major() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(
        root,
        "crates/demo/src/lib.rs",
        "pub fn api3() {}\n",
        "refactor(demo): reshape",
    );
    // Amend in a BREAKING CHANGE footer via a follow-up commit body.
    git(
        root,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "fix: tidy\n\nBREAKING CHANGE: dropped a field",
        ],
    );

    let decision = decide(root, "origin/main..HEAD").unwrap();
    assert_eq!(decision.to, "5.0.0");
    assert_eq!(decision.bump, "major");
}

#[test]
fn skip_version_latest_commit_yields_no_bump() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(
        root,
        "crates/demo/src/feature.rs",
        "pub fn added() {}\n",
        "feat(demo): a feature",
    );
    // The engine's own release commit is the latest commit.
    git(
        root,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "chore(release): v4.1.0 [skip-version]",
        ],
    );

    let decision = decide(root, "origin/main..HEAD").unwrap();
    assert!(decision.skipped);
    assert_eq!(decision.from, decision.to);
    assert_eq!(decision.bump, "none");

    // apply on a skipped decision is a no-op: version + CHANGELOG untouched.
    let commits = commits_in_range(root, "origin/main..HEAD").unwrap();
    apply(root, &decision, &commits).unwrap();
    assert_eq!(read_workspace_version(root).unwrap().to_string(), "4.0.0");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(!changelog.contains("## v4.1.0"));
}

#[test]
fn commits_in_range_excludes_skip_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(root, "a.txt", "a\n", "feat: real");
    git(
        root,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "chore(release): v9.9.9 [skip-version]",
        ],
    );

    let commits = commits_in_range(root, "origin/main..HEAD").unwrap();
    // Only the real feat commit survives; the release commit is filtered out.
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].kind, "feat");
}
