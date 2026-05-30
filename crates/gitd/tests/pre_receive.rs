mod common;

use gitd::hooks::PreReceiveGuard;
use gitd::object_fsck::ObjectFsck;
use gitd::protection::ProtectedRefRule;
use gitd::{GitdConfig, RepoId, RepoManager};

#[test]
fn pre_receive_blocks_main_delete_before_fsck_matters() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jitforge-prereceive");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo = manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}")))
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    let guard = PreReceiveGuard::new(
        ProtectedRefRule::default_phase1_rules(),
        ObjectFsck::new("git"),
    );
    let input = "1111111111111111111111111111111111111111 0000000000000000000000000000000000000000 refs/heads/main\n";
    assert!(guard.evaluate_lines(&repo, "alice", input).is_err());
    let _ = std::fs::remove_dir_all(root);
}
