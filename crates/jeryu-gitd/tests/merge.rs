mod common;

use jeryu_gitd::error::GitdError;
use jeryu_gitd::refs::RefService;
use jeryu_gitd::repo::Repository;
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::path::PathBuf;
use std::process::Command;

const MERGE_ACTOR: &str = "system:pr-merge";

#[test]
fn merge_pull_fast_forward_advances_base() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-merge-ff");
    let base_oid = fixture.main_oid.clone();
    // A commit directly on top of main: base is its ancestor => pure FF.
    let head_oid = commit_on_top(&fixture.work, &fixture.repo, "feature", "line two\n");

    let svc = RefService::new(fixture.manager.clone());
    let outcome = svc
        .merge_pull(
            &fixture.repo,
            MERGE_ACTOR,
            "refs/heads/main",
            &base_oid,
            &head_oid,
            "Merge pull request #1",
            false,
        )
        .unwrap_or_else(|err| panic!("merge_pull failed: {err}"));

    assert!(outcome.fast_forward, "expected a fast-forward merge");
    assert_eq!(outcome.merge_oid, head_oid);
    assert_eq!(ref_oid(&svc, &fixture.repo, "refs/heads/main"), head_oid);
    fixture.cleanup();
}

#[test]
fn merge_pull_creates_real_merge_commit_when_diverged() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-merge-diverged");
    // Advance main one commit so the head will diverge from the new base.
    let new_base = commit_on_top(&fixture.work, &fixture.repo, "main", "base advance\n");
    assert_eq!(new_base, fixture.main_oid_after_push());

    // Build a divergent head off the ORIGINAL main, touching a different file
    // so the merge is clean (no conflict).
    checkout_detached(&fixture.work, &fixture.main_oid);
    let head_oid = commit_file_on_top(
        &fixture.work,
        &fixture.repo,
        "feature",
        "NOTES.md",
        "diverged note\n",
    );

    let svc = RefService::new(fixture.manager.clone());
    let outcome = svc
        .merge_pull(
            &fixture.repo,
            MERGE_ACTOR,
            "refs/heads/main",
            &new_base,
            &head_oid,
            "Merge pull request #1",
            false,
        )
        .unwrap_or_else(|err| panic!("merge_pull failed: {err}"));

    assert!(!outcome.fast_forward, "expected a true merge commit");
    // The merge oid is a real commit object.
    assert!(rev_parse_commit(&fixture.repo, &outcome.merge_oid));
    // It has exactly two parents: [new_base, head].
    let parents = rev_list_parents(&fixture.repo, &outcome.merge_oid);
    assert_eq!(parents, vec![new_base.clone(), head_oid.clone()]);
    // main now points at the merge commit, and new_base is its ancestor (proves
    // a non-force advance).
    assert_eq!(
        ref_oid(&svc, &fixture.repo, "refs/heads/main"),
        outcome.merge_oid
    );
    assert!(is_ancestor(&fixture.repo, &new_base, &outcome.merge_oid));
    fixture.cleanup();
}

#[test]
fn merge_pull_conflict_is_rejected_and_base_unchanged() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-merge-conflict");
    // Advance main editing README.md.
    let new_base = commit_file_on_top(
        &fixture.work,
        &fixture.repo,
        "main",
        "README.md",
        "base edit\n",
    );

    // Divergent head edits the SAME file differently => true conflict.
    checkout_detached(&fixture.work, &fixture.main_oid);
    let head_oid = commit_file_on_top(
        &fixture.work,
        &fixture.repo,
        "feature",
        "README.md",
        "head edit\n",
    );

    let svc = RefService::new(fixture.manager.clone());
    let err = svc
        .merge_pull(
            &fixture.repo,
            MERGE_ACTOR,
            "refs/heads/main",
            &new_base,
            &head_oid,
            "Merge pull request #1",
            false,
        )
        .unwrap_err();

    assert!(
        matches!(err, GitdError::MergeConflict(_)),
        "expected MergeConflict, got {err:?}"
    );
    // No partial advance, no ref move.
    assert_eq!(ref_oid(&svc, &fixture.repo, "refs/heads/main"), new_base);
    fixture.cleanup();
}

#[test]
fn merge_pull_rejects_synthetic_sha() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-merge-synthetic");
    let base_oid = fixture.main_oid.clone();

    let svc = RefService::new(fixture.manager.clone());
    let err = svc
        .merge_pull(
            &fixture.repo,
            MERGE_ACTOR,
            "refs/heads/main",
            &base_oid,
            "merge-abc-1",
            "Merge pull request #1",
            false,
        )
        .unwrap_err();

    assert!(
        matches!(err, GitdError::InvalidInput(_)),
        "expected InvalidInput for a synthetic sha, got {err:?}"
    );
    assert_eq!(ref_oid(&svc, &fixture.repo, "refs/heads/main"), base_oid);
    fixture.cleanup();
}

#[test]
fn merge_pull_refuses_non_ff_when_linear_required() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-merge-linear");
    let new_base = commit_on_top(&fixture.work, &fixture.repo, "main", "base advance\n");

    checkout_detached(&fixture.work, &fixture.main_oid);
    let head_oid = commit_file_on_top(
        &fixture.work,
        &fixture.repo,
        "feature",
        "NOTES.md",
        "diverged note\n",
    );

    let svc = RefService::new(fixture.manager.clone());
    let err = svc
        .merge_pull(
            &fixture.repo,
            MERGE_ACTOR,
            "refs/heads/main",
            &new_base,
            &head_oid,
            "Merge pull request #1",
            true, // require_fast_forward
        )
        .unwrap_err();

    assert!(
        matches!(err, GitdError::NonFastForwardRequired),
        "expected NonFastForwardRequired, got {err:?}"
    );
    // Base must be untouched (no commit created, no ref moved).
    assert_eq!(ref_oid(&svc, &fixture.repo, "refs/heads/main"), new_base);
    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// Fixture and git helpers (mirrors crates/jeryu-gitd/tests/refs.rs)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct BareFixture {
    root: PathBuf,
    work: PathBuf,
    manager: RepoManager,
    repo: Repository,
    main_oid: String,
}

impl BareFixture {
    fn cleanup(self) {
        let _ = std::fs::remove_dir_all(self.root);
        let _ = std::fs::remove_dir_all(self.work);
    }

    fn main_oid_after_push(&self) -> String {
        rev_parse(&self.repo, "refs/heads/main")
    }
}

fn seed_bare_with_main(prefix: &str) -> BareFixture {
    let root = common::temp_dir(&format!("{prefix}-root"));
    let work = common::temp_dir(&format!("{prefix}-work"));
    let manager = RepoManager::new(GitdConfig::new(&root));
    let id = RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}"));
    let repo = manager
        .create_bare(&id)
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    run_git(&work, &["init"], "git init");
    run_git(
        &work,
        &["config", "user.email", "test@example.invalid"],
        "git config email",
    );
    run_git(&work, &["config", "user.name", "Test"], "git config name");
    std::fs::write(work.join("README.md"), "hello\n")
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git(&work, &["add", "README.md"], "git add");
    run_git(&work, &["commit", "-m", "seed"], "git commit");
    push(&work, &repo, "HEAD", "refs/heads/main");
    let main_oid = rev_parse_head(&work);
    BareFixture {
        root,
        work,
        manager,
        repo,
        main_oid,
    }
}

/// Commit a new line to README.md on top of the current HEAD and push to a
/// scratch branch; returns the new commit oid.
fn commit_on_top(work: &PathBuf, repo: &Repository, branch: &str, contents: &str) -> String {
    commit_file_on_top(work, repo, branch, "README.md", contents)
}

/// Commit `contents` to `file` on top of the current HEAD and push to a scratch
/// branch in the bare repo; returns the new commit oid.
fn commit_file_on_top(
    work: &PathBuf,
    repo: &Repository,
    branch: &str,
    file: &str,
    contents: &str,
) -> String {
    std::fs::write(work.join(file), contents).unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git(work, &["add", file], "git add");
    run_git(work, &["commit", "-m", "update"], "git commit");
    push(work, repo, "HEAD", &format!("refs/heads/{branch}"));
    rev_parse_head(work)
}

fn checkout_detached(work: &PathBuf, oid: &str) {
    run_git(work, &["checkout", "--detach", oid], "git checkout detach");
}

fn push(work: &PathBuf, repo: &Repository, src: &str, dst: &str) {
    run_git(
        work,
        &[
            "push",
            "--force",
            repo.path.to_str().unwrap_or_default(),
            &format!("{src}:{dst}"),
        ],
        "git push",
    );
}

fn ref_oid(svc: &RefService, repo: &Repository, name: &str) -> String {
    svc.list_refs(repo)
        .unwrap_or_else(|err| panic!("list_refs failed: {err}"))
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("ref {name} not found"))
        .oid
}

fn rev_parse(repo: &Repository, refname: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", refname])
        .current_dir(&repo.path)
        .output()
        .unwrap_or_else(|err| panic!("git rev-parse failed: {err}"));
    assert!(
        output.status.success(),
        "git rev-parse {refname} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn rev_parse_commit(repo: &Repository, oid: &str) -> bool {
    Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{oid}^{{commit}}"),
        ])
        .current_dir(&repo.path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn rev_list_parents(repo: &Repository, oid: &str) -> Vec<String> {
    let output = Command::new("git")
        .args(["rev-list", "--parents", "-n", "1", oid])
        .current_dir(&repo.path)
        .output()
        .unwrap_or_else(|err| panic!("git rev-list failed: {err}"));
    assert!(output.status.success(), "git rev-list failed");
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // First token is the commit itself; the rest are its parents.
    line.split_whitespace().skip(1).map(String::from).collect()
}

fn is_ancestor(repo: &Repository, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(&repo.path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn rev_parse_head(work: &PathBuf) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(work)
        .output()
        .unwrap_or_else(|err| panic!("git rev-parse failed: {err}"));
    assert!(
        output.status.success(),
        "git rev-parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn run_git(work: &PathBuf, args: &[&str], label: &str) {
    let status = Command::new("git")
        .args(args)
        .current_dir(work)
        .status()
        .unwrap_or_else(|err| panic!("{label} failed to start: {err}"));
    assert!(status.success(), "{label} failed with {status}");
}
