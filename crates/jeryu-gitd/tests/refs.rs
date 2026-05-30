mod common;

use jeryu_gitd::refs::{RefService, ZERO_OID, validate_ref_name};
use jeryu_gitd::repo::Repository;
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::path::PathBuf;
use std::process::Command;

#[test]
fn refs_lists_created_branch() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-refs-list");
    let refs = RefService::new(fixture.manager.clone())
        .list_refs(&fixture.repo)
        .unwrap_or_else(|err| panic!("refs failed: {err}"));
    assert!(refs.iter().any(|r| r.name == "refs/heads/main"));
    fixture.cleanup();
}

#[test]
fn ref_service_denies_protected_main_delete_before_mutation() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-refs-protected-delete");
    let err = RefService::new(fixture.manager.clone())
        .update_ref(
            &fixture.repo,
            "alice",
            "refs/heads/main",
            ZERO_OID,
            Some(&fixture.main_oid),
        )
        .unwrap_err();

    assert!(err.to_string().contains("cannot delete"));
    let refs = RefService::new(fixture.manager.clone())
        .list_refs(&fixture.repo)
        .unwrap_or_else(|read_err| panic!("refs failed after denied delete: {read_err}"));
    assert!(
        refs.iter()
            .any(|r| r.name == "refs/heads/main" && r.oid == fixture.main_oid)
    );
    fixture.cleanup();
}

#[test]
fn ref_service_denies_protected_main_non_fast_forward_before_mutation() {
    if !common::git_available() {
        return;
    }
    let mut fixture = seed_bare_with_main("jeryu-refs-protected-force");
    let first_oid = fixture.main_oid.clone();
    fixture.main_oid = commit_and_push(&fixture.work, &fixture.repo, "second\n");
    let second_oid = fixture.main_oid.clone();

    let err = RefService::new(fixture.manager.clone())
        .update_ref(
            &fixture.repo,
            "alice",
            "refs/heads/main",
            &first_oid,
            Some(&second_oid),
        )
        .unwrap_err();

    assert!(err.to_string().contains("cannot force-update"));
    let refs = RefService::new(fixture.manager.clone())
        .list_refs(&fixture.repo)
        .unwrap_or_else(|read_err| panic!("refs failed after denied update: {read_err}"));
    assert!(
        refs.iter()
            .any(|r| r.name == "refs/heads/main" && r.oid == second_oid)
    );
    fixture.cleanup();
}

#[test]
fn ref_name_validation_rejects_command_like_and_nul_names() {
    if !common::git_available() {
        return;
    }
    assert!(validate_ref_name("-refs/heads/main").is_err());
    assert!(validate_ref_name("refs/heads/main\0shadow").is_err());
}

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
    run_git(
        &work,
        &[
            "push",
            repo.path.to_str().unwrap_or_default(),
            "HEAD:refs/heads/main",
        ],
        "git push",
    );
    let main_oid = rev_parse_head(&work);
    BareFixture {
        root,
        work,
        manager,
        repo,
        main_oid,
    }
}

fn commit_and_push(work: &PathBuf, repo: &Repository, contents: &str) -> String {
    std::fs::write(work.join("README.md"), contents)
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git(work, &["add", "README.md"], "git add");
    run_git(work, &["commit", "-m", "update"], "git commit");
    run_git(
        work,
        &[
            "push",
            repo.path.to_str().unwrap_or_default(),
            "HEAD:refs/heads/main",
        ],
        "git push",
    );
    rev_parse_head(work)
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
