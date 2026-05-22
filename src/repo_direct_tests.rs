use super::repo_direct_gitlab::seed_source_ref;
use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::tempdir;

fn git(repo_root: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout={}\nstderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn configure_git_hooks_sets_repo_local_hooks_path() {
    let repo = tempdir().expect("temp repo");
    git(repo.path(), &["init"]);

    let hooks_dir = repo.path().join("ops/git-hooks");
    fs::create_dir_all(&hooks_dir).expect("create hooks dir");
    let source_hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("ops/git-hooks/pre-push");
    let target_hook = hooks_dir.join("pre-push");
    fs::copy(&source_hook, &target_hook).expect("copy hook");
    let mut perms = fs::metadata(&target_hook)
        .expect("hook metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&target_hook, perms).expect("set hook perms");

    configure_git_hooks(repo.path()).expect("configure hooks");

    let output = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["config", "--local", "--get", "core.hooksPath"])
        .output()
        .expect("git config read");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "ops/git-hooks"
    );
    assert!(repo.path().join("ops/git-hooks/pre-push").is_file());
}

#[test]
fn direct_mode_unsets_local_hooks_path() {
    let repo = tempdir().expect("temp repo");
    git(repo.path(), &["init"]);
    git(
        repo.path(),
        &["config", "--local", "core.hooksPath", "ops/git-hooks"],
    );

    configure_hook_mode(repo.path(), HookMode::Off, HookProfile::All).unwrap();

    let output = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["config", "--local", "--get", "core.hooksPath"])
        .output()
        .expect("git config read");
    assert!(!output.status.success());
}

#[test]
fn observed_and_enforced_modes_install_expected_hooks() {
    let repo = tempdir().expect("temp repo");
    git(repo.path(), &["init"]);

    configure_hook_mode(
        repo.path(),
        HookMode::Advisory,
        HookProfile::PreCommitJankurai,
    )
    .unwrap();
    let pre_commit = fs::read_to_string(repo.path().join(".jeryu/hooks/pre-commit")).unwrap();
    assert!(pre_commit.contains("jankurai audit . --changed-fast"));
    assert!(pre_commit.contains("exit 0"));

    configure_hook_mode(repo.path(), HookMode::Enforce, HookProfile::PrePush).unwrap();
    let pre_push = fs::read_to_string(repo.path().join(".jeryu/hooks/pre-push")).unwrap();
    assert!(pre_push.contains("ops/ci/quality-gates.sh"));
    assert!(pre_push.contains("exit $status"));

    let output = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["config", "--local", "--get", "core.hooksPath"])
        .output()
        .expect("git config read");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        ".jeryu/hooks"
    );
}

#[test]
fn existing_repo_preserves_origin_and_adds_jeryu_remote() {
    let repo = tempdir().expect("temp repo");
    git(repo.path(), &["init"]);
    git(
        repo.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@example.invalid:team/demo.git",
        ],
    );

    configure_remote(
        repo.path(),
        "jeryu",
        "ssh://git@127.0.0.1:2224/team/demo.git",
        false,
    )
    .unwrap();

    let origin = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["remote", "get-url", "origin"])
        .output()
        .unwrap();
    let jeryu = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["remote", "get-url", "jeryu"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&origin.stdout).trim(),
        "git@example.invalid:team/demo.git"
    );
    assert_eq!(
        String::from_utf8_lossy(&jeryu.stdout).trim(),
        "ssh://git@127.0.0.1:2224/team/demo.git"
    );
}

#[test]
fn replace_origin_removes_other_remotes() {
    let repo = tempdir().expect("temp repo");
    git(repo.path(), &["init"]);
    git(
        repo.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@example.invalid:team/demo.git",
        ],
    );
    git(
        repo.path(),
        &[
            "remote",
            "add",
            "backup",
            "git@example.invalid:team/backup.git",
        ],
    );

    configure_remote(
        repo.path(),
        "origin",
        "ssh://git@127.0.0.1:2224/team/demo.git",
        true,
    )
    .unwrap();
    remove_other_remotes(repo.path(), "origin").unwrap();

    let output = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["remote"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "origin");
}

#[test]
fn new_repo_uses_origin_as_local_jeryu_remote() {
    let repo = tempdir().expect("temp repo");
    git(repo.path(), &["init"]);

    configure_remote(
        repo.path(),
        "origin",
        "ssh://git@127.0.0.1:2224/team/demo.git",
        true,
    )
    .unwrap();

    let origin = std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["remote", "get-url", "origin"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&origin.stdout).trim(),
        "ssh://git@127.0.0.1:2224/team/demo.git"
    );
}

#[test]
fn seed_source_prefers_origin_main() {
    let repo = tempdir().expect("temp repo");
    git(repo.path(), &["init", "-b", "main"]);
    git(
        repo.path(),
        &["config", "user.email", "jeryu@example.invalid"],
    );
    git(repo.path(), &["config", "user.name", "JeRyu Test"]);
    fs::write(repo.path().join("README.md"), "demo").unwrap();
    git(repo.path(), &["add", "README.md"]);
    git(repo.path(), &["commit", "-m", "init"]);
    git(
        repo.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );

    assert_eq!(
        seed_source_ref(repo.path(), "main").unwrap().as_deref(),
        Some("refs/remotes/origin/main")
    );
}

#[test]
fn jeryu_toml_rendering_is_deterministic_and_secret_free() {
    let repo = tempdir().expect("temp repo");
    write_jeryu_configs(
        repo.path(),
        JeryuConfigSpec {
            mode: RepoMode::Observed,
            hooks: HookMode::Advisory,
            namespace: "team",
            name: "demo",
            branch: "main",
            protect_main: true,
            main_relay: true,
            offline_release_remote: Some("https://github.com/neverhuman/warp"),
        },
    )
    .unwrap();
    let first = fs::read_to_string(repo.path().join(".jeryu/policy.toml")).unwrap();
    write_jeryu_configs(
        repo.path(),
        JeryuConfigSpec {
            mode: RepoMode::Observed,
            hooks: HookMode::Advisory,
            namespace: "team",
            name: "demo",
            branch: "main",
            protect_main: true,
            main_relay: true,
            offline_release_remote: Some("https://github.com/neverhuman/warp"),
        },
    )
    .unwrap();
    let second = fs::read_to_string(repo.path().join(".jeryu/policy.toml")).unwrap();
    assert_eq!(first, second);
    assert!(first.contains("[main_relay]"));
    assert!(first.contains("actor = \"jeryu\""));
    assert!(first.contains("[offline_release_mirror]"));
    let combined = ["repo.toml", "policy.toml", "backup.toml", "ci.toml"]
        .iter()
        .map(|name| fs::read_to_string(repo.path().join(".jeryu").join(name)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!combined.to_ascii_lowercase().contains("token"));
    assert!(!combined.to_ascii_lowercase().contains("password"));
    assert!(!combined.to_ascii_lowercase().contains("identityfile"));
}
