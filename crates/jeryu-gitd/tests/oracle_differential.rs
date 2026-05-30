mod common;

use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn gitd_managed_repo_matches_stock_bare_git_semantics() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-git-oracle-diff-root");
    let seed = common::temp_dir("jeryu-git-oracle-diff-seed");
    let stock = root.join("stock.git");

    run_command(
        Command::new("git").arg("init").arg("--bare").arg(&stock),
        "stock bare init",
    );
    let manager = RepoManager::new(GitdConfig::new(root.join("jeryu")));
    let jeryu = manager
        .create_bare(&RepoId::new("oracle", "demo").unwrap_or_else(|err| panic!("repo id: {err}")))
        .unwrap_or_else(|err| panic!("create jeryu bare repo: {err}"));

    seed_repository(&seed);
    push_all_refs(&seed, &stock, "stock");
    push_all_refs(&seed, &jeryu.path, "jeryu");

    run_git(&stock, &["fsck", "--strict"], "stock fsck");
    run_git(&jeryu.path, &["fsck", "--strict"], "jeryu fsck");

    let stock_refs = ref_listing(&stock);
    let jeryu_refs = ref_listing(&jeryu.path);
    assert_eq!(jeryu_refs, stock_refs);
    for line in stock_refs.lines() {
        let oid = line
            .split_whitespace()
            .nth(1)
            .unwrap_or_else(|| panic!("malformed ref listing: {line}"));
        assert_eq!(
            git_output(&jeryu.path, &["cat-file", "-t", oid]),
            git_output(&stock, &["cat-file", "-t", oid])
        );
        assert_eq!(
            git_output(&jeryu.path, &["cat-file", "-p", oid]),
            git_output(&stock, &["cat-file", "-p", oid])
        );
    }

    let stock_clone = root.join("stock-clone");
    let jeryu_clone = root.join("jeryu-clone");
    clone_main(&stock, &stock_clone, "stock clone");
    clone_main(&jeryu.path, &jeryu_clone, "jeryu clone");
    assert_eq!(
        git_output(&jeryu_clone, &["rev-parse", "HEAD"]),
        git_output(&stock_clone, &["rev-parse", "HEAD"])
    );
    assert_eq!(
        git_output(&jeryu_clone, &["ls-files", "-s"]),
        git_output(&stock_clone, &["ls-files", "-s"])
    );
    assert_eq!(
        std::fs::read_to_string(jeryu_clone.join("README.md")).unwrap_or_default(),
        std::fs::read_to_string(stock_clone.join("README.md")).unwrap_or_default()
    );
    assert_eq!(
        std::fs::read_to_string(jeryu_clone.join("crates/demo/src/lib.rs")).unwrap_or_default(),
        std::fs::read_to_string(stock_clone.join("crates/demo/src/lib.rs")).unwrap_or_default()
    );

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(seed);
}

fn seed_repository(work: &Path) {
    run_git(work, &["init"], "seed init");
    run_git(
        work,
        &["config", "user.email", "oracle@example.invalid"],
        "seed email",
    );
    run_git(work, &["config", "user.name", "Git Oracle"], "seed name");
    std::fs::create_dir_all(work.join("crates/demo/src"))
        .unwrap_or_else(|err| panic!("create fixture dirs: {err}"));
    std::fs::write(work.join("README.md"), "oracle\n")
        .unwrap_or_else(|err| panic!("write readme: {err}"));
    std::fs::write(
        work.join("crates/demo/src/lib.rs"),
        "pub fn demo() -> bool { true }\n",
    )
    .unwrap_or_else(|err| panic!("write lib: {err}"));
    run_git(work, &["add", "."], "seed add");
    run_git(work, &["commit", "-m", "seed"], "seed commit");
    run_git(work, &["branch", "-M", "main"], "seed branch main");
    run_git(work, &["checkout", "-b", "topic"], "seed topic branch");
    std::fs::write(work.join("topic.txt"), "topic\n")
        .unwrap_or_else(|err| panic!("write topic: {err}"));
    run_git(work, &["add", "topic.txt"], "topic add");
    run_git(work, &["commit", "-m", "topic"], "topic commit");
    run_git(work, &["checkout", "main"], "seed checkout main");
    run_git(work, &["tag", "v1.0.0"], "seed tag");
}

fn push_all_refs(work: &Path, remote: &Path, label: &str) {
    let remote = remote.to_str().unwrap_or_default();
    run_git(
        work,
        &["push", remote, "main:refs/heads/main"],
        &format!("{label} push main"),
    );
    run_git(
        work,
        &["push", remote, "topic:refs/heads/topic"],
        &format!("{label} push topic"),
    );
    run_git(
        work,
        &["push", remote, "v1.0.0:refs/tags/v1.0.0"],
        &format!("{label} push tag"),
    );
}

fn clone_main(remote: &Path, destination: &PathBuf, label: &str) {
    run_command(
        Command::new("git")
            .arg("clone")
            .arg("--branch")
            .arg("main")
            .arg(remote)
            .arg(destination),
        label,
    );
}

fn ref_listing(repo: &Path) -> String {
    git_output(
        repo,
        &[
            "for-each-ref",
            "--sort=refname",
            "--format=%(refname) %(objectname)",
        ],
    )
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
