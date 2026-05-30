mod common;

use jeryu_gitd::hooks::PreReceiveGuard;
use jeryu_gitd::object_fsck::ObjectFsck;
use jeryu_gitd::protection::ProtectedRefRule;
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};

#[test]
fn pre_receive_blocks_main_delete_before_fsck_matters() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-prereceive");
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

#[test]
fn pre_receive_rejects_invalid_ref_name_before_accepting_change() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-prereceive-invalid-ref");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo = manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}")))
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    let guard = PreReceiveGuard::new(Vec::new(), ObjectFsck::new("git"));
    let input = "0000000000000000000000000000000000000000 1111111111111111111111111111111111111111 -bad-ref\n";

    let err = guard.evaluate_lines(&repo, "alice", input).unwrap_err();

    assert!(err.to_string().contains("invalid ref name"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn pre_receive_rejects_short_or_non_hex_oids() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-prereceive-invalid-oid");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo = manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}")))
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    let guard = PreReceiveGuard::new(Vec::new(), ObjectFsck::new("git"));
    let short_oid = "0 1111111111111111111111111111111111111111 refs/heads/topic\n";
    let non_hex_oid = "1111111111111111111111111111111111111111 zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz refs/heads/topic\n";

    assert!(
        guard
            .evaluate_lines(&repo, "alice", short_oid)
            .unwrap_err()
            .to_string()
            .contains("invalid prior oid")
    );
    assert!(
        guard
            .evaluate_lines(&repo, "alice", non_hex_oid)
            .unwrap_err()
            .to_string()
            .contains("invalid next oid")
    );
    let _ = std::fs::remove_dir_all(root);
}
