//! B2 e2e: a GATED PR merge advances `refs/heads/main` in the real bare repo,
//! while a blocked PR does NOT move main.
//!
//! Wires `GithubRouter::with_core(core).with_repo_manager(rm)` over a temp bare
//! repo seeded with real base/head commits and drives the merge through the
//! GitHub-compatible REST edge.
#![cfg(feature = "web")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jeryu_api::GithubRouter;
use jeryu_core::{CreateReviewRequest, ForgeCore, ReviewState};
use jeryu_gitd::refs::RefService;
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use serde_json::Value;

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn temp_dir(prefix: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "{}-{}",
        prefix,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&base).expect("create temp dir");
    base
}

fn run_git(dir: &Path, args: &[&str], label: &str) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap_or_else(|err| panic!("{label} failed to start: {err}"));
    assert!(status.success(), "{label} failed with {status}");
}

fn rev_parse_head(work: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(work)
        .output()
        .expect("git rev-parse");
    assert!(out.status.success(), "rev-parse failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn body(resp: &jeryu_api::Response) -> Value {
    serde_json::from_str(&resp.body)
        .unwrap_or_else(|err| panic!("bad json body: {err}: {}", resp.body))
}

fn is_hex40(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Seed a bare `acme/demo.git` with a `main` commit and a fast-forward feature
/// commit. Returns the manager, the storage root, the work dir, and the two
/// real oids.
struct GitFixture {
    root: PathBuf,
    work: PathBuf,
    manager: Arc<RepoManager>,
    base_oid: String,
    head_oid: String,
}

impl GitFixture {
    fn cleanup(self) {
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = std::fs::remove_dir_all(&self.work);
    }

    fn main_ref(&self) -> String {
        let id = RepoId::new("acme", "demo").unwrap();
        let repo = self.manager.resolve(&id).unwrap();
        RefService::new((*self.manager).clone())
            .list_refs(&repo)
            .unwrap()
            .into_iter()
            .find(|r| r.name == "refs/heads/main")
            .map(|r| r.oid)
            .expect("main ref present")
    }
}

fn seed_fixture(prefix: &str) -> GitFixture {
    let root = temp_dir(&format!("{prefix}-root"));
    let work = temp_dir(&format!("{prefix}-work"));
    let manager = Arc::new(RepoManager::new(GitdConfig::new(&root)));
    let id = RepoId::new("acme", "demo").unwrap();
    let repo = manager.create_bare(&id).expect("create bare");

    run_git(&work, &["init"], "git init");
    run_git(
        &work,
        &["config", "user.email", "test@example.invalid"],
        "config email",
    );
    run_git(&work, &["config", "user.name", "Test"], "config name");
    std::fs::write(work.join("README.md"), "hello\n").expect("write");
    run_git(&work, &["add", "README.md"], "git add");
    run_git(&work, &["commit", "-m", "seed"], "git commit");
    run_git(
        &work,
        &["push", repo.path.to_str().unwrap(), "HEAD:refs/heads/main"],
        "push main",
    );
    let base_oid = rev_parse_head(&work);

    // A fast-forward feature commit on top of main.
    std::fs::write(work.join("FEATURE.md"), "feature\n").expect("write");
    run_git(&work, &["add", "FEATURE.md"], "git add");
    run_git(&work, &["commit", "-m", "feature"], "git commit");
    run_git(
        &work,
        &[
            "push",
            repo.path.to_str().unwrap(),
            "HEAD:refs/heads/feature",
        ],
        "push feature",
    );
    let head_oid = rev_parse_head(&work);

    GitFixture {
        root,
        work,
        manager,
        base_oid,
        head_oid,
    }
}

/// Build a router over a forge core with the repo created and the bare repo
/// wired, plus a PR opened with the REAL base/head oids.
fn router_with_pr(fixture: &GitFixture) -> (GithubRouter, u64) {
    let core = ForgeCore::new();
    let router = GithubRouter::with_core(core).with_repo_manager(fixture.manager.clone());

    let created = router.post(
        "/repos",
        r#"{"owner":"acme","name":"demo","private":false,"default_branch":"main"}"#,
    );
    assert_eq!(created.status, 201, "create repo: {}", created.body);

    let opened = router.post(
        "/repos/acme/demo/pulls",
        &format!(
            r#"{{"title":"feature","head":"feature","base":"main","head_sha":"{}","base_sha":"{}","actor":"alice"}}"#,
            fixture.head_oid, fixture.base_oid
        ),
    );
    assert_eq!(opened.status, 201, "open pr: {}", opened.body);
    let number = body(&opened)["number"].as_u64().expect("pr number");

    // Require one approving review on main.
    let protect = router.put(
        "/repos/acme/demo/branches/main/protection",
        r#"{"required_approving_review_count":1}"#,
    );
    assert_eq!(protect.status, 200, "set protection: {}", protect.body);

    (router, number)
}

#[test]
fn gated_merge_moves_main_in_bare_repo() {
    if !git_available() {
        return;
    }
    let fixture = seed_fixture("jeryu-merge-gate-pass");
    let (router, number) = router_with_pr(&fixture);

    // Approve so the gate passes.
    router
        .core()
        .create_review(
            "acme",
            "demo",
            number,
            "bob",
            CreateReviewRequest {
                body: None,
                event: ReviewState::Approved,
                comments: vec![],
            },
        )
        .expect("approve");

    let merged = router.put(&format!("/repos/acme/demo/pulls/{number}/merge"), "{}");
    assert_eq!(merged.status, 200, "merge: {}", merged.body);
    let merge_body = body(&merged);
    assert_eq!(merge_body["merged"], true);
    let sha = merge_body["sha"].as_str().expect("sha");
    assert!(is_hex40(sha), "sha should be a real 40-hex oid, got {sha}");
    // Fast-forward: the merge sha IS the head commit.
    assert_eq!(sha, fixture.head_oid);

    // MAIN MOVED in the real bare repo.
    assert_eq!(
        fixture.main_ref(),
        fixture.head_oid,
        "main must advance to head"
    );

    // The PR record now carries the real merge sha.
    let after = router.get(&format!("/repos/acme/demo/pulls/{number}"));
    assert_eq!(after.status, 200);
    let after_body = body(&after);
    assert_eq!(after_body["merged"], true);
    assert_eq!(after_body["merge_commit_sha"], fixture.head_oid);

    fixture.cleanup();
}

/// Bare repo where `main` has advanced one commit (the new base) and a clean,
/// DIVERGED head exists off the original seed — so merging produces a real
/// two-parent merge commit, NOT a fast-forward. `base_oid` is the advanced main.
fn seed_diverged_fixture(prefix: &str) -> GitFixture {
    let root = temp_dir(&format!("{prefix}-root"));
    let work = temp_dir(&format!("{prefix}-work"));
    let manager = Arc::new(RepoManager::new(GitdConfig::new(&root)));
    let id = RepoId::new("acme", "demo").unwrap();
    let repo = manager.create_bare(&id).expect("create bare");

    run_git(&work, &["init"], "git init");
    run_git(
        &work,
        &["config", "user.email", "test@example.invalid"],
        "config email",
    );
    run_git(&work, &["config", "user.name", "Test"], "config name");
    std::fs::write(work.join("README.md"), "hello\n").expect("write");
    run_git(&work, &["add", "README.md"], "git add");
    run_git(&work, &["commit", "-m", "seed"], "git commit");
    run_git(
        &work,
        &["push", repo.path.to_str().unwrap(), "HEAD:refs/heads/main"],
        "push main",
    );
    let seed_oid = rev_parse_head(&work);

    // Advance main one commit (the new base).
    std::fs::write(work.join("README.md"), "base advance\n").expect("write");
    run_git(&work, &["add", "README.md"], "git add");
    run_git(&work, &["commit", "-m", "advance"], "git commit");
    run_git(
        &work,
        &["push", repo.path.to_str().unwrap(), "HEAD:refs/heads/main"],
        "push main advance",
    );
    let new_base = rev_parse_head(&work);

    // Diverged head off the ORIGINAL seed, touching a different file (clean merge).
    run_git(
        &work,
        &["checkout", "--detach", &seed_oid],
        "checkout detach",
    );
    std::fs::write(work.join("NOTES.md"), "note\n").expect("write");
    run_git(&work, &["add", "NOTES.md"], "git add");
    run_git(&work, &["commit", "-m", "note"], "git commit");
    run_git(
        &work,
        &[
            "push",
            repo.path.to_str().unwrap(),
            "HEAD:refs/heads/feature",
        ],
        "push feature",
    );
    let head_oid = rev_parse_head(&work);

    GitFixture {
        root,
        work,
        manager,
        base_oid: new_base,
        head_oid,
    }
}

/// Parents of `oid` in the bare `acme/demo` repo via `git rev-list --parents -n1`.
fn parents_of(manager: &RepoManager, oid: &str) -> Vec<String> {
    let id = RepoId::new("acme", "demo").unwrap();
    let repo = manager.resolve(&id).unwrap();
    let out = Command::new("git")
        .args(["rev-list", "--parents", "-n", "1", oid])
        .current_dir(&repo.path)
        .output()
        .expect("git rev-list");
    assert!(out.status.success(), "rev-list failed");
    let line = String::from_utf8_lossy(&out.stdout);
    let mut toks = line.split_whitespace().map(|s| s.to_string());
    let _commit = toks.next(); // first token is the commit oid itself
    toks.collect()
}

#[test]
fn gated_true_merge_creates_merge_commit_and_moves_main() {
    if !git_available() {
        return;
    }
    // PRIMARY B2 PROOF for the non-fast-forward path: a gated+approved PR with a
    // diverged head merges into a real TWO-PARENT merge commit that advances main.
    let fixture = seed_diverged_fixture("jeryu-merge-true");
    let (router, number) = router_with_pr(&fixture); // requires 1 approving review

    router
        .core()
        .create_review(
            "acme",
            "demo",
            number,
            "bob",
            CreateReviewRequest {
                body: None,
                event: ReviewState::Approved,
                comments: vec![],
            },
        )
        .expect("approve");

    assert_eq!(
        fixture.main_ref(),
        fixture.base_oid,
        "main starts at the advanced base"
    );

    let merged = router.put(&format!("/repos/acme/demo/pulls/{number}/merge"), "{}");
    assert_eq!(merged.status, 200, "true merge: {}", merged.body);
    let merge_body = body(&merged);
    assert_eq!(merge_body["merged"], true);
    let sha = merge_body["sha"].as_str().expect("sha");
    assert!(is_hex40(sha), "real 40-hex merge oid, got {sha}");
    // A true merge commit is a NEW object: neither the head nor the base.
    assert_ne!(
        sha, fixture.head_oid,
        "true merge sha must differ from head"
    );
    assert_ne!(
        sha, fixture.base_oid,
        "true merge sha must differ from base"
    );

    // main advanced to the new merge commit in the REAL bare repo.
    assert_eq!(
        fixture.main_ref(),
        sha,
        "main must advance to the merge commit"
    );

    // The merge commit has exactly two parents: [base, head].
    let parents = parents_of(&fixture.manager, sha);
    assert_eq!(
        parents.len(),
        2,
        "merge commit must have two parents, got {parents:?}"
    );
    assert!(
        parents.contains(&fixture.base_oid),
        "parent set must include the base"
    );
    assert!(
        parents.contains(&fixture.head_oid),
        "parent set must include the head"
    );

    // The PR record carries the real merge sha (checkout-able), not a synthetic one.
    let after = router.get(&format!("/repos/acme/demo/pulls/{number}"));
    assert_eq!(after.status, 200);
    let after_body = body(&after);
    assert_eq!(after_body["merged"], true);
    assert_eq!(after_body["merge_commit_sha"], sha);

    fixture.cleanup();
}

#[test]
fn blocked_pr_does_not_move_main() {
    if !git_available() {
        return;
    }
    let fixture = seed_fixture("jeryu-merge-gate-block");
    let (router, number) = router_with_pr(&fixture);

    // NO approving review: protection requires 1, so the PR is blocked.
    let main_before = fixture.main_ref();
    assert_eq!(main_before, fixture.base_oid);

    let merged = router.put(&format!("/repos/acme/demo/pulls/{number}/merge"), "{}");
    assert_eq!(merged.status, 405, "blocked merge: {}", merged.body);

    // Main did NOT move.
    assert_eq!(
        fixture.main_ref(),
        fixture.base_oid,
        "main must NOT advance"
    );

    // The PR is not merged.
    let after = router.get(&format!("/repos/acme/demo/pulls/{number}"));
    assert_eq!(after.status, 200);
    assert_eq!(body(&after)["merged"], false);

    fixture.cleanup();
}

#[test]
fn linear_history_base_refuses_true_merge_and_main_unchanged() {
    if !git_available() {
        return;
    }
    // Diverged head + required_linear_history=true => the real merge primitive
    // refuses the non-fast-forward merge (409) and main must not move.
    let root = temp_dir("jeryu-merge-linear-root");
    let work = temp_dir("jeryu-merge-linear-work");
    let manager = Arc::new(RepoManager::new(GitdConfig::new(&root)));
    let id = RepoId::new("acme", "demo").unwrap();
    let repo = manager.create_bare(&id).expect("create bare");

    run_git(&work, &["init"], "git init");
    run_git(
        &work,
        &["config", "user.email", "test@example.invalid"],
        "config email",
    );
    run_git(&work, &["config", "user.name", "Test"], "config name");
    std::fs::write(work.join("README.md"), "hello\n").expect("write");
    run_git(&work, &["add", "README.md"], "git add");
    run_git(&work, &["commit", "-m", "seed"], "git commit");
    run_git(
        &work,
        &["push", repo.path.to_str().unwrap(), "HEAD:refs/heads/main"],
        "push main",
    );
    let seed_oid = rev_parse_head(&work);

    // Advance main one commit (new base).
    std::fs::write(work.join("README.md"), "base advance\n").expect("write");
    run_git(&work, &["add", "README.md"], "git add");
    run_git(&work, &["commit", "-m", "advance"], "git commit");
    run_git(
        &work,
        &["push", repo.path.to_str().unwrap(), "HEAD:refs/heads/main"],
        "push main advance",
    );
    let new_base = rev_parse_head(&work);

    // Diverged head off the original seed, touching a different file (clean).
    run_git(
        &work,
        &["checkout", "--detach", &seed_oid],
        "checkout detach",
    );
    std::fs::write(work.join("NOTES.md"), "note\n").expect("write");
    run_git(&work, &["add", "NOTES.md"], "git add");
    run_git(&work, &["commit", "-m", "note"], "git commit");
    run_git(
        &work,
        &[
            "push",
            repo.path.to_str().unwrap(),
            "HEAD:refs/heads/feature",
        ],
        "push feature",
    );
    let head_oid = rev_parse_head(&work);

    let core = ForgeCore::new();
    let router = GithubRouter::with_core(core).with_repo_manager(manager.clone());
    let created = router.post(
        "/repos",
        r#"{"owner":"acme","name":"demo","private":false,"default_branch":"main"}"#,
    );
    assert_eq!(created.status, 201, "create repo: {}", created.body);
    let opened = router.post(
        "/repos/acme/demo/pulls",
        &format!(
            r#"{{"title":"diverged","head":"feature","base":"main","head_sha":"{head_oid}","base_sha":"{new_base}","actor":"alice"}}"#
        ),
    );
    assert_eq!(opened.status, 201, "open pr: {}", opened.body);
    let number = body(&opened)["number"].as_u64().expect("pr number");

    // Protect main with linear history (and no review requirement so only the
    // FF-only rule blocks the merge).
    let protect = router.put(
        "/repos/acme/demo/branches/main/protection",
        r#"{"required_linear_history":true}"#,
    );
    assert_eq!(protect.status, 200, "set protection: {}", protect.body);

    let merged = router.put(&format!("/repos/acme/demo/pulls/{number}/merge"), "{}");
    assert_eq!(
        merged.status, 409,
        "non-ff merge on linear base must be refused: {}",
        merged.body
    );

    // Main did NOT move off the new base.
    let main_now = RefService::new((*manager).clone())
        .list_refs(&repo)
        .unwrap()
        .into_iter()
        .find(|r| r.name == "refs/heads/main")
        .map(|r| r.oid)
        .expect("main ref present");
    assert_eq!(main_now, new_base, "main must NOT advance");

    let after = router.get(&format!("/repos/acme/demo/pulls/{number}"));
    assert_eq!(body(&after)["merged"], false);

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&work);
}
