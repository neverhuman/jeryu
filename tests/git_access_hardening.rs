use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

fn init_git_repo(repo_path: &Path, origin: &str) {
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo_path)
        .status()
        .unwrap();
    assert!(status.success());
    let status = Command::new("git")
        .args(["remote", "add", "origin", origin])
        .current_dir(repo_path)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn git_passthrough_rejects_local_http_gitlab_origins_with_repair_hint() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_git_repo(&repo, "http://localhost:8929/root/jekko.git");

    let output = Command::new(env!("CARGO_BIN_EXE_jeryu"))
        .arg("git")
        .arg("status")
        .current_dir(&repo)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(
        "jeryu git: local GitLab HTTP origins are forbidden; run `jeryu access repair --repo . --yes`"
    ));
}
