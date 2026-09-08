mod common;

use jeryu_gitd::import::{GitImportAction, LocalGitImporter};
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::path::Path;
use std::process::Command;

#[test]
fn local_import_materializes_cloneable_and_fetchable_gitd_repo() {
    if !common::git_available() {
        return;
    }

    let storage = common::temp_dir("jeryu-import-storage");
    let source = common::temp_dir("jeryu-import-source");
    let clone = common::temp_dir("jeryu-import-clone");

    run_git(&source, &["init"], "source init");
    run_git(
        &source,
        &["config", "user.email", "import@example.invalid"],
        "source email",
    );
    run_git(
        &source,
        &["config", "user.name", "Git Import"],
        "source name",
    );
    std::fs::write(source.join("README.md"), "imported v1\n")
        .unwrap_or_else(|err| panic!("write source readme: {err}"));
    run_git(&source, &["add", "README.md"], "source add");
    run_git(&source, &["commit", "-m", "seed"], "source commit");
    run_git(&source, &["branch", "-M", "main"], "source main branch");

    let manager = RepoManager::new(GitdConfig::new(&storage));
    let importer = LocalGitImporter::new(manager.clone());
    let report = importer
        .import_paths(Some("local"), std::slice::from_ref(&source))
        .unwrap_or_else(|err| panic!("import source repo: {err}"));

    assert_eq!(report.imported.len(), 1);
    assert!(report.skipped.is_empty());
    assert_eq!(report.imported[0].action, GitImportAction::Cloned);
    assert!(
        Path::new(&report.imported[0].repo_path)
            .join("HEAD")
            .is_file()
    );
    assert!(
        Path::new(&report.imported[0].repo_path)
            .join("jeryu/repo-id")
            .is_file()
    );

    let repo = manager
        .open(&RepoId::new("local", &report.imported[0].name).unwrap())
        .unwrap_or_else(|err| panic!("open imported repo: {err}"));
    run_command(
        Command::new("git")
            .arg("clone")
            .arg("--branch")
            .arg("main")
            .arg(&repo.path)
            .arg(&clone),
        "clone imported gitd repo",
    );
    assert_eq!(
        std::fs::read_to_string(clone.join("README.md")).unwrap_or_default(),
        "imported v1\n"
    );

    std::fs::write(source.join("README.md"), "imported v1\nimported v2\n")
        .unwrap_or_else(|err| panic!("write updated source readme: {err}"));
    run_git(&source, &["commit", "-am", "update"], "source update");
    let fetch_report = importer
        .import_paths(Some("local"), std::slice::from_ref(&source))
        .unwrap_or_else(|err| panic!("fetch source repo: {err}"));
    assert_eq!(fetch_report.imported[0].action, GitImportAction::Fetched);

    run_git(&clone, &["fetch", "origin", "main"], "clone fetch imported");
    run_git(
        &clone,
        &["merge", "--ff-only", "FETCH_HEAD"],
        "clone merge imported",
    );
    assert_eq!(
        std::fs::read_to_string(clone.join("README.md")).unwrap_or_default(),
        "imported v1\nimported v2\n"
    );

    let _ = std::fs::remove_dir_all(storage);
    let _ = std::fs::remove_dir_all(source);
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
