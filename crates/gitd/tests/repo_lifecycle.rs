mod common;

use gitd::{GitdConfig, RepoId, RepoManager};

#[test]
fn repo_create_and_open() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jitforge-repo");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let id = RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}"));
    let repo = manager
        .create_bare(&id)
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    assert!(repo.path.join("HEAD").is_file());
    let opened = manager
        .open(&id)
        .unwrap_or_else(|err| panic!("open failed: {err}"));
    assert_eq!(repo.path, opened.path);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn repo_rejects_traversal() {
    assert!(RepoId::new("..", "demo").is_err());
    assert!(RepoId::new("acme", "../demo").is_err());
}
