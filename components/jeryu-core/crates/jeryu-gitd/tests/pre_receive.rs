mod common;

use jeryu_gitd::hooks::PreReceiveGuard;
use jeryu_gitd::object_fsck::ObjectFsck;
use jeryu_gitd::protection::ProtectedRefRule;
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::path::Path;
use std::process::Command;

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
fn pre_receive_blocks_direct_main_update_before_fsck_matters() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-prereceive-main-update");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo = manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}")))
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    let guard = PreReceiveGuard::new(
        ProtectedRefRule::default_phase1_rules(),
        ObjectFsck::new("git"),
    );
    let input = "1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 refs/heads/main\n";

    let err = guard.evaluate_lines(&repo, "alice", input).unwrap_err();

    assert!(
        err.to_string()
            .contains("direct pushes to refs/heads/main are blocked"),
        "unexpected error: {err}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn installed_pre_receive_hook_blocks_direct_wire_push_to_main() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-prereceive-hook-root");
    let work = common::temp_dir("jeryu-prereceive-hook-work");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo = manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}")))
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    manager
        .install_pre_receive_hook(&repo)
        .unwrap_or_else(|err| panic!("install hook failed: {err}"));

    run_git_ok(&work, &["init"], "git init");
    run_git_ok(
        &work,
        &["config", "user.email", "test@example.invalid"],
        "git config email",
    );
    run_git_ok(&work, &["config", "user.name", "Test"], "git config name");
    std::fs::write(work.join("README.md"), "hello\n")
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git_ok(&work, &["add", "README.md"], "git add");
    run_git_ok(&work, &["commit", "-m", "seed"], "git commit");

    let output = git_with_installed_hook(
        &work,
        &repo.path,
        &[
            "push",
            repo.path.to_str().unwrap_or_default(),
            "HEAD:refs/heads/main",
        ],
        false,
    );

    assert!(
        !output.status.success(),
        "direct main push unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("direct pushes to refs/heads/main are blocked"),
        "unexpected stderr: {stderr}"
    );
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn installed_hook_keeps_tags_write_once_without_gitd_on_path() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-prereceive-immutable-tag-root");
    let work = common::temp_dir("jeryu-prereceive-immutable-tag-work");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo = manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}")))
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    manager
        .install_pre_receive_hook(&repo)
        .unwrap_or_else(|err| panic!("install hook failed: {err}"));

    run_git_ok(&work, &["init"], "git init");
    run_git_ok(
        &work,
        &["config", "user.email", "test@example.invalid"],
        "git config email",
    );
    run_git_ok(&work, &["config", "user.name", "Test"], "git config name");
    std::fs::write(work.join("README.md"), "first\n")
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git_ok(&work, &["add", "README.md"], "git add");
    run_git_ok(&work, &["commit", "-m", "first"], "git commit");
    let first = git_output(&work, &["rev-parse", "HEAD"]);
    run_git_ok(&work, &["tag", "v1.0.0"], "git tag");

    let repo_path = repo.path.to_string_lossy().to_string();
    run_git_with_installed_hook_ok(
        &work,
        &repo.path,
        &[
            "push",
            &repo_path,
            "HEAD:refs/heads/topic",
            "refs/tags/v1.0.0:refs/tags/v1.0.0",
        ],
        "initial branch and tag push",
    );

    std::fs::write(work.join("README.md"), "second\n")
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git_ok(&work, &["add", "README.md"], "git add second");
    run_git_ok(&work, &["commit", "-m", "second"], "git commit second");
    run_git_ok(&work, &["tag", "-f", "v1.0.0"], "move local tag");

    let update = git_with_installed_hook(
        &work,
        &repo.path,
        &["push", "--force", &repo_path, "refs/tags/v1.0.0"],
        true,
    );
    assert!(!update.status.success(), "immutable tag update succeeded");
    assert!(
        String::from_utf8_lossy(&update.stderr)
            .contains("immutable tags cannot be updated or deleted"),
        "unexpected update stderr: {}",
        String::from_utf8_lossy(&update.stderr)
    );

    let delete = git_with_installed_hook(
        &work,
        &repo.path,
        &["push", &repo_path, ":refs/tags/v1.0.0"],
        true,
    );
    assert!(!delete.status.success(), "immutable tag deletion succeeded");
    assert!(
        String::from_utf8_lossy(&delete.stderr)
            .contains("immutable tags cannot be updated or deleted"),
        "unexpected delete stderr: {}",
        String::from_utf8_lossy(&delete.stderr)
    );

    assert_eq!(
        git_output(&repo.path, &["rev-parse", "refs/tags/v1.0.0"]),
        first
    );
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(work);
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

fn run_git_ok(work: &Path, args: &[&str], label: &str) {
    let status = Command::new("git")
        .args(args)
        .current_dir(work)
        .status()
        .unwrap_or_else(|err| panic!("{label} failed to start: {err}"));
    assert!(status.success(), "{label} failed with {status}");
}

fn git_with_installed_hook(
    work: &Path,
    repo: &Path,
    args: &[&str],
    without_gitd: bool,
) -> std::process::Output {
    let hooks = repo.join("hooks");
    let mut command = Command::new("git");
    // Release workers suppress checkout hooks with command-scoped Git config.
    // Reproduce that hostile setting, then bind only this fixture's installed hook.
    command
        .args(args)
        .current_dir(work)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "2")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", "/dev/null")
        .env("GIT_CONFIG_KEY_1", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_1", hooks);
    if without_gitd {
        command
            .env("PATH", "/usr/bin:/bin")
            .env_remove("JERYU_GITD_BIN");
    }
    command
        .output()
        .unwrap_or_else(|err| panic!("git {args:?} failed to start: {err}"))
}

fn run_git_with_installed_hook_ok(work: &Path, repo: &Path, args: &[&str], label: &str) {
    let output = git_with_installed_hook(work, repo, args, true);
    assert!(
        output.status.success(),
        "{label} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn object_fsck_rejects_raw_blob_above_limit_with_lfs_guidance() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-prereceive-large-blob-root");
    let work = common::temp_dir("jeryu-prereceive-large-blob-work");
    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo = manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}")))
        .unwrap_or_else(|err| panic!("create failed: {err}"));

    run_git_ok(&work, &["init"], "git init");
    run_git_ok(
        &work,
        &["config", "user.email", "test@example.invalid"],
        "git config email",
    );
    run_git_ok(&work, &["config", "user.name", "Test"], "git config name");
    std::fs::write(work.join("model.cpkt"), vec![7u8; 2048])
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git_ok(&work, &["add", "model.cpkt"], "git add");
    run_git_ok(&work, &["commit", "-m", "raw model"], "git commit");
    let commit = git_output(&work, &["rev-parse", "HEAD"]);
    let repo_path = repo.path.to_string_lossy().to_string();
    run_git_ok(
        &work,
        &["push", &repo_path, "HEAD:refs/heads/tmp"],
        "git push temp ref",
    );
    let status = Command::new("git")
        .args([
            "--git-dir",
            &repo_path,
            "update-ref",
            "-d",
            "refs/heads/tmp",
        ])
        .status()
        .unwrap_or_else(|err| panic!("delete temp ref failed to start: {err}"));
    assert!(status.success(), "delete temp ref failed with {status}");

    let err = ObjectFsck::new("git")
        .reject_oversized_raw_blobs(&repo, &[commit], 1024)
        .unwrap_err();

    let message = err.to_string();
    assert!(
        message.contains("model.cpkt")
            && message.contains("exceeding GitHub's 100 MiB limit")
            && message.contains("Git LFS"),
        "unexpected error: {message}"
    );
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(work);
}

fn git_output(work: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(work)
        .output()
        .unwrap_or_else(|err| panic!("git {args:?} failed to start: {err}"));
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
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
