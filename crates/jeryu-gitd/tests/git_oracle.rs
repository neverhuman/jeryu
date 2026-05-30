mod common;

use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::path::Path;
use std::process::Command;

#[test]
fn stock_git_clone_fetch_and_push_round_trips_through_gitd_repo() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-git-oracle-root");
    let seed = common::temp_dir("jeryu-git-oracle-seed");
    let clone = common::temp_dir("jeryu-git-oracle-clone");

    let manager = RepoManager::new(GitdConfig::new(&root));
    let repo_id = RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("repo id: {err}"));
    let repo = manager
        .create_bare(&repo_id)
        .unwrap_or_else(|err| panic!("create bare repo: {err}"));

    run_git(&seed, &["init"], "seed init");
    run_git(
        &seed,
        &["config", "user.email", "oracle@example.invalid"],
        "seed email",
    );
    run_git(&seed, &["config", "user.name", "Git Oracle"], "seed name");
    std::fs::write(seed.join("README.md"), "seed\n")
        .unwrap_or_else(|err| panic!("write seed readme: {err}"));
    run_git(&seed, &["add", "README.md"], "seed add");
    run_git(&seed, &["commit", "-m", "seed"], "seed commit");
    run_git(
        &seed,
        &[
            "push",
            repo.path.to_str().unwrap_or_default(),
            "HEAD:refs/heads/main",
        ],
        "seed push",
    );

    run_command(
        Command::new("git")
            .arg("clone")
            .arg("--branch")
            .arg("main")
            .arg(repo.path.to_str().unwrap_or_default())
            .arg(&clone),
        "clone from gitd-managed repo",
    );
    run_git(
        &clone,
        &["config", "user.email", "oracle@example.invalid"],
        "clone email",
    );
    run_git(&clone, &["config", "user.name", "Git Oracle"], "clone name");
    assert_eq!(
        std::fs::read_to_string(clone.join("README.md")).unwrap_or_default(),
        "seed\n"
    );

    std::fs::write(seed.join("README.md"), "seed\nupstream\n")
        .unwrap_or_else(|err| panic!("write upstream readme: {err}"));
    run_git(&seed, &["commit", "-am", "upstream"], "upstream commit");
    run_git(
        &seed,
        &[
            "push",
            repo.path.to_str().unwrap_or_default(),
            "HEAD:refs/heads/main",
        ],
        "upstream push",
    );
    run_git(&clone, &["fetch", "origin", "main"], "clone fetch");
    run_git(
        &clone,
        &["merge", "--ff-only", "FETCH_HEAD"],
        "clone ff merge",
    );
    assert!(
        std::fs::read_to_string(clone.join("README.md"))
            .unwrap_or_default()
            .contains("upstream")
    );

    std::fs::write(clone.join("README.md"), "seed\nupstream\nclient\n")
        .unwrap_or_else(|err| panic!("write client readme: {err}"));
    run_git(&clone, &["commit", "-am", "client"], "client commit");
    run_git(&clone, &["push", "origin", "HEAD:main"], "client push");

    let remote_head = git_output(&repo.path, &["rev-parse", "refs/heads/main"]);
    let client_head = git_output(&clone, &["rev-parse", "HEAD"]);
    assert_eq!(remote_head, client_head);

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(seed);
    let _ = std::fs::remove_dir_all(clone);
}

fn run_git(work: &Path, args: &[&str], label: &str) {
    run_command(Command::new("git").args(args).current_dir(work), label);
}

fn run_command(command: &mut Command, label: &str) {
    let output = command
        .output()
        .unwrap_or_else(|err| panic!("{label} failed to start: {err}"));
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output(work: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(work)
        .output()
        .unwrap_or_else(|err| panic!("git output failed: {err}"));
    assert!(
        output.status.success(),
        "git output failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
