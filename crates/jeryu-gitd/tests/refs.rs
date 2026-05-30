mod common;

use jeryu_gitd::refs::RefService;
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::process::Command;

#[test]
fn refs_lists_created_branch() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-refs-root");
    let work = common::temp_dir("jeryu-refs-work");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let id = RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}"));
    let repo = manager
        .create_bare(&id)
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    Command::new("git")
        .args(["init"])
        .current_dir(&work)
        .status()
        .unwrap_or_else(|err| panic!("git init failed: {err}"));
    Command::new("git")
        .args(["config", "user.email", "test@example.invalid"])
        .current_dir(&work)
        .status()
        .unwrap_or_else(|err| panic!("git config failed: {err}"));
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&work)
        .status()
        .unwrap_or_else(|err| panic!("git config failed: {err}"));
    std::fs::write(work.join("README.md"), "hello\n")
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&work)
        .status()
        .unwrap_or_else(|err| panic!("git add failed: {err}"));
    Command::new("git")
        .args(["commit", "-m", "seed"])
        .current_dir(&work)
        .status()
        .unwrap_or_else(|err| panic!("git commit failed: {err}"));
    Command::new("git")
        .args([
            "push",
            repo.path.to_str().unwrap_or_default(),
            "HEAD:refs/heads/main",
        ])
        .current_dir(&work)
        .status()
        .unwrap_or_else(|err| panic!("git push failed: {err}"));
    let refs = RefService::new(manager)
        .list_refs(&repo)
        .unwrap_or_else(|err| panic!("refs failed: {err}"));
    assert!(refs.iter().any(|r| r.name == "refs/heads/main"));
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(work);
}
