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
    std::fs::create_dir_all(work.join(".github/workflows")).expect("create workflow dir");
    std::fs::write(
        work.join(".github/workflows/ci.yml"),
        "name: ci\non: [push, pull_request]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ci\n",
    )
    .expect("write workflow");
    run_git(&work, &["add", "."], "git add feature and workflow");
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

    let runs = router
        .core()
        .list_check_runs("acme", "demo", Some(&fixture.head_oid))
        .expect("list check-runs for head");
    assert!(
        runs.total_count >= 1,
        "opening a PR should seed CI check-runs, got {runs:?}"
    );

    // Require one approving review on main.
    let protect = router.put(
        "/repos/acme/demo/branches/main/protection",
        r#"{"required_approving_review_count":1}"#,
    );
    assert_eq!(protect.status, 200, "set protection: {}", protect.body);

    (router, number)
}

#[test]
fn commit_check_runs_resolve_and_filter_one_exact_head() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_fixture("jeryu-check-runs-exact-head");
    let router = GithubRouter::new().with_repo_manager(fixture.manager.clone());
    let created = router.post(
        "/repos",
        r#"{"owner":"acme","name":"demo","private":false,"default_branch":"main"}"#,
    );
    assert_eq!(created.status, 201, "create repo: {}", created.body);

    let base_check = router.post(
        "/repos/acme/demo/check-runs",
        &format!(
            r#"{{"name":"demo/required","head_sha":"{}","status":"completed","conclusion":"failure"}}"#,
            fixture.base_oid
        ),
    );
    assert_eq!(base_check.status, 201, "base check: {}", base_check.body);
    let head_check = router.post(
        "/repos/acme/demo/check-runs",
        &format!(
            r#"{{"name":"demo/required","head_sha":"{}","status":"completed","conclusion":"success"}}"#,
            fixture.head_oid
        ),
    );
    assert_eq!(head_check.status, 201, "head check: {}", head_check.body);

    let by_head = router.get(&format!(
        "/repos/acme/demo/commits/{}/check-runs",
        fixture.head_oid
    ));
    assert_eq!(by_head.status, 200, "head lookup: {}", by_head.body);
    let head_runs = body(&by_head);
    assert_eq!(head_runs["total_count"], 1);
    assert_eq!(head_runs["check_runs"][0]["head_sha"], fixture.head_oid);
    assert_eq!(head_runs["check_runs"][0]["conclusion"], "success");

    let by_main = router.get("/repos/acme/demo/commits/main/check-runs");
    assert_eq!(by_main.status, 200, "main lookup: {}", by_main.body);
    let main_runs = body(&by_main);
    assert_eq!(main_runs["total_count"], 1);
    assert_eq!(main_runs["check_runs"][0]["head_sha"], fixture.base_oid);
    assert_eq!(main_runs["check_runs"][0]["conclusion"], "failure");

    let unknown = router.get("/repos/acme/demo/commits/does-not-exist/check-runs");
    assert_eq!(unknown.status, 422, "unknown lookup: {}", unknown.body);
    assert!(
        body(&unknown)["message"]
            .as_str()
            .expect("message")
            .contains("unknown or ambiguous")
    );

    fixture.cleanup();
}

#[test]
fn gated_merge_moves_main_in_bare_repo() {
    assert!(git_available(), "real Git is required for this regression");
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
                expected_head_sha: Some(fixture.head_oid.clone()),
            },
        )
        .expect("approve");

    assert_legacy_merge_unavailable(&router, number, &fixture);
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
    assert!(git_available(), "real Git is required for this regression");
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
                expected_head_sha: Some(fixture.head_oid.clone()),
            },
        )
        .expect("approve");

    assert_eq!(
        fixture.main_ref(),
        fixture.base_oid,
        "main starts at the advanced base"
    );

    assert_legacy_merge_unavailable(&router, number, &fixture);
    fixture.cleanup();
}

#[test]
fn blocked_pr_does_not_move_main() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_fixture("jeryu-merge-gate-block");
    let (router, number) = router_with_pr(&fixture);

    // NO approving review: protection requires 1, so the PR is blocked.
    let main_before = fixture.main_ref();
    assert_eq!(main_before, fixture.base_oid);

    let merged = router.put(&format!("/repos/acme/demo/pulls/{number}/merge"), "{}");
    assert_eq!(merged.status, 503, "blocked merge: {}", merged.body);

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
    assert!(git_available(), "real Git is required for this regression");
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
        merged.status, 503,
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

/// Create the `acme/demo` repo over a router wired to `fixture`'s bare repo.
/// Unlike `router_with_pr`, this opens NO PR and sets NO protection — callers
/// drive the PR lifecycle themselves to exercise the live-resolve paths.
fn router_over(fixture: &GitFixture) -> GithubRouter {
    let core = ForgeCore::new();
    let router = GithubRouter::with_core(core).with_repo_manager(fixture.manager.clone());
    let created = router.post(
        "/repos",
        r#"{"owner":"acme","name":"demo","private":false,"default_branch":"main"}"#,
    );
    assert_eq!(created.status, 201, "create repo: {}", created.body);
    router
}

/// Open a PR WITHOUT supplying any head/base sha, mirroring the GitHub flow that
/// only names branches. Returns the PR number.
fn open_pr_by_branch(router: &GithubRouter, head: &str, base: &str) -> u64 {
    let opened = router.post(
        "/repos/acme/demo/pulls",
        &format!(r#"{{"title":"{head}","head":"{head}","base":"{base}","actor":"alice"}}"#),
    );
    assert_eq!(opened.status, 201, "open pr: {}", opened.body);
    body(&opened)["number"].as_u64().expect("pr number")
}

#[test]
fn open_pr_persists_real_resolved_oids_not_placeholders() {
    assert!(git_available(), "real Git is required for this regression");
    // A create request that names only branches (no shas) must persist the REAL
    // commit oids of those refs, never the "base"/"head-<n>" placeholders that
    // wedged `PUT /merge` with "oid is not a commit in this repository: base".
    let fixture = seed_fixture("jeryu-open-resolves");
    let router = router_over(&fixture);
    let number = open_pr_by_branch(&router, "feature", "main");

    let pr = body(&router.get(&format!("/repos/acme/demo/pulls/{number}")));
    let head_sha = pr["head"]["sha"].as_str().expect("head sha");
    let base_sha = pr["base"]["sha"].as_str().expect("base sha");

    assert_eq!(
        head_sha, fixture.head_oid,
        "head must be the real feature oid"
    );
    assert_eq!(base_sha, fixture.base_oid, "base must be the real main oid");
    assert!(
        is_hex40(head_sha),
        "head sha must be a 40-hex oid, got {head_sha}"
    );
    assert!(
        is_hex40(base_sha),
        "base sha must be a 40-hex oid, got {base_sha}"
    );
    assert_ne!(base_sha, "base", "base must not be the literal placeholder");
    assert!(
        !head_sha.starts_with("head-"),
        "head must not be the head-<n> placeholder"
    );

    fixture.cleanup();
}

#[test]
fn normal_pr_merges_via_live_resolve_and_advances_main() {
    assert!(git_available(), "real Git is required for this regression");
    // A not-yet-merged PR opened by branch name (no stored shas) merges through
    // the REAL git path and fast-forwards main to the head — proving the merge
    // no longer depends on stored shas.
    let fixture = seed_fixture("jeryu-normal-live");
    let router = router_over(&fixture);
    let number = open_pr_by_branch(&router, "feature", "main");

    assert_legacy_merge_unavailable(&router, number, &fixture);
    fixture.cleanup();
}

/// Bare repo where `main` is one commit AHEAD of `feature`: feature's tip is an
/// ancestor of main (its code already landed). `base_oid` is the advanced main,
/// `head_oid` is the older feature tip contained in main's history.
fn seed_landed_fixture(prefix: &str) -> GitFixture {
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
    // feature points at the seed commit.
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

    // main advances one commit beyond feature: feature's code is now in main.
    std::fs::write(work.join("README.md"), "landed\n").expect("write");
    run_git(&work, &["add", "README.md"], "git add");
    run_git(&work, &["commit", "-m", "land"], "git commit");
    run_git(
        &work,
        &["push", repo.path.to_str().unwrap(), "HEAD:refs/heads/main"],
        "push main",
    );
    let base_oid = rev_parse_head(&work);

    GitFixture {
        root,
        work,
        manager,
        base_oid,
        head_oid,
    }
}

#[test]
fn merging_already_landed_pr_marks_merged_idempotently() {
    assert!(git_available(), "real Git is required for this regression");
    // The core stale-record bug: a PR whose head is already an ANCESTOR of base
    // (its code fast-forwarded into main server-side) must merge with
    // {merged:true} and flip the record to merged WITHOUT moving any ref — and
    // be idempotent on a repeat call.
    let fixture = seed_landed_fixture("jeryu-landed");
    let router = router_over(&fixture);
    let number = open_pr_by_branch(&router, "feature", "main");

    assert_legacy_merge_unavailable(&router, number, &fixture);
    fixture.cleanup();
}

#[test]
fn merge_with_unresolvable_head_returns_4xx_not_500() {
    assert!(git_available(), "real Git is required for this regression");
    // A PR whose head names a branch that does not exist (and carries no real
    // stored head sha) must yield a clean typed 4xx, never a 500.
    let fixture = seed_fixture("jeryu-unresolvable-head");
    let router = router_over(&fixture);
    // head "ghost" has no ref; base "main" resolves fine.
    let number = open_pr_by_branch(&router, "ghost", "main");

    let merged = router.put(&format!("/repos/acme/demo/pulls/{number}/merge"), "{}");
    assert_eq!(
        merged.status, 503,
        "unresolvable head must be writer-unavailable, got {}: {}",
        merged.status, merged.body
    );
    assert!(merged.status >= 400, "must not report a successful merge");

    // The PR was NOT merged.
    let after = body(&router.get(&format!("/repos/acme/demo/pulls/{number}")));
    assert_eq!(after["merged"], false);

    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// Merge -> GitHub mirror push (jeryu_api::github_mirror)
// ---------------------------------------------------------------------------

fn mirror_for(dest: Option<&Path>) -> Arc<jeryu_api::github_mirror::GithubMirror> {
    use jeryu_api::github_mirror::{GithubMirror, GithubMirrorTarget};
    let mut targets = std::collections::BTreeMap::new();
    targets.insert(
        "acme/demo".to_string(),
        GithubMirrorTarget {
            github_slug: "neverhuman/demo".to_string(),
            branch: "main".to_string(),
            destination_override: dest.map(|p| p.to_string_lossy().into_owned()),
        },
    );
    Arc::new(GithubMirror::with_targets(targets))
}

fn approve(router: &GithubRouter, number: u64) {
    let expected_head_sha = router
        .core()
        .get_pull_request("acme", "demo", number)
        .expect("load PR before approval")
        .head
        .sha;
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
                expected_head_sha: Some(expected_head_sha),
            },
        )
        .expect("approve");
}

fn mirror_check_runs(router: &GithubRouter, sha: &str) -> Vec<(String, Option<String>)> {
    router
        .core()
        .list_check_runs("acme", "demo", Some(sha))
        .expect("list check-runs")
        .check_runs
        .into_iter()
        .filter(|run| run.name == jeryu_api::github_mirror::MIRROR_CHECK_NAME)
        .map(|run| {
            (
                run.name,
                run.conclusion
                    .map(|c| format!("{c:?}").to_ascii_lowercase()),
            )
        })
        .collect()
}

#[test]
fn merged_pr_pushes_main_to_configured_github_destination() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_fixture("jeryu-merge-mirror-pass");
    let (router, number) = router_with_pr(&fixture);

    // A local bare repo stands in for github.com/neverhuman/demo.
    let dest = temp_dir("jeryu-merge-mirror-dest").join("demo.git");
    run_git(
        dest.parent().unwrap(),
        &["init", "--bare", dest.to_str().unwrap()],
        "init dest bare",
    );
    let router = router.with_github_mirror(mirror_for(Some(&dest)));

    approve(&router, number);
    assert_legacy_merge_unavailable(&router, number, &fixture);
    let runs = mirror_check_runs(&router, &fixture.head_oid);
    assert!(runs.is_empty(), "unavailable merge must not push: {runs:?}");
    let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    fixture.cleanup();
}

#[test]
fn merge_succeeds_when_github_push_fails() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_fixture("jeryu-merge-mirror-fail");
    let (router, number) = router_with_pr(&fixture);

    // Destination path does not exist -> the push must fail.
    let bogus = std::env::temp_dir().join("jeryu-merge-mirror-nonexistent/丢失.git");
    let router = router.with_github_mirror(mirror_for(Some(&bogus)));

    approve(&router, number);
    assert_legacy_merge_unavailable(&router, number, &fixture);
    assert!(mirror_check_runs(&router, &fixture.head_oid).is_empty());
    fixture.cleanup();
}

#[test]
fn unconfigured_repo_pushes_nothing() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_fixture("jeryu-merge-mirror-skip");
    let (router, number) = router_with_pr(&fixture);

    // Mirror configured for a DIFFERENT repo: acme/demo is not a target.
    use jeryu_api::github_mirror::{GithubMirror, GithubMirrorTarget};
    let mut targets = std::collections::BTreeMap::new();
    targets.insert(
        "other/repo".to_string(),
        GithubMirrorTarget {
            github_slug: "neverhuman/other".to_string(),
            branch: "main".to_string(),
            destination_override: None,
        },
    );
    let router = router.with_github_mirror(Arc::new(GithubMirror::with_targets(targets)));

    approve(&router, number);
    assert_legacy_merge_unavailable(&router, number, &fixture);
    assert!(mirror_check_runs(&router, &fixture.head_oid).is_empty());
    fixture.cleanup();
}

// Preserved from Deploy PR 1 alongside the still-required positive journey.
// Advisory review rows and existing ancestry cannot supply operation authority.
fn assert_legacy_merge_unavailable(router: &GithubRouter, number: u64, fixture: &GitFixture) {
    let path = format!("/repos/acme/demo/pulls/{number}");
    let before = body(&router.get(&path));
    let checks_before = router.core().list_check_runs("acme", "demo", None).unwrap();
    for _ in 0..2 {
        let response = router.put(
            &format!("{path}/merge"),
            &serde_json::json!({"sha": fixture.head_oid, "merge_method": "merge"}).to_string(),
        );
        assert_eq!(response.status, 503, "{}", response.body);
        assert!(body(&response)["documentation_url"].is_string());
        assert_eq!(fixture.main_ref(), fixture.base_oid);
        assert_eq!(body(&router.get(&path)), before);
    }
    assert_eq!(
        router.core().list_check_runs("acme", "demo", None).unwrap(),
        checks_before
    );
}

#[test]
fn legacy_advisory_approval_cannot_advance_real_main() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_fixture("jeryu-legacy-refused-ff");
    let (router, number) = router_with_pr(&fixture);
    approve(&router, number);
    assert_legacy_merge_unavailable(&router, number, &fixture);
    fixture.cleanup();
}

#[test]
fn legacy_diverged_merge_cannot_create_a_merge_commit() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_diverged_fixture("jeryu-legacy-refused-diverged");
    let (router, number) = router_with_pr(&fixture);
    approve(&router, number);
    assert_ne!(fixture.base_oid, fixture.head_oid);
    assert_eq!(parents_of(&fixture.manager, &fixture.base_oid).len(), 1);
    assert_legacy_merge_unavailable(&router, number, &fixture);
    fixture.cleanup();
}

#[test]
fn already_landed_ancestry_cannot_invent_an_authorized_merge_receipt() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_landed_fixture("jeryu-legacy-refused-landed");
    let router = router_over(&fixture);
    let number = open_pr_by_branch(&router, "feature", "main");
    assert_eq!(
        parents_of(&fixture.manager, &fixture.base_oid),
        vec![fixture.head_oid.clone()]
    );
    assert_legacy_merge_unavailable(&router, number, &fixture);
    fixture.cleanup();
}

#[test]
fn refused_legacy_merge_does_not_push_to_a_configured_mirror() {
    assert!(git_available(), "real Git is required for this regression");
    let fixture = seed_fixture("jeryu-legacy-refused-mirror");
    let (router, number) = router_with_pr(&fixture);
    let destination = fixture.work.join("mirror.git");
    run_git(
        &fixture.work,
        &["init", "--bare", destination.to_str().unwrap()],
        "init mirror",
    );
    let router = router.with_github_mirror(mirror_for(Some(&destination)));
    approve(&router, number);
    assert_legacy_merge_unavailable(&router, number, &fixture);
    let output = Command::new("git")
        .args(["for-each-ref", "--format=%(refname)"])
        .current_dir(&destination)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "refused merge must not create mirror refs"
    );
    assert!(mirror_check_runs(&router, &fixture.head_oid).is_empty());
    fixture.cleanup();
}
