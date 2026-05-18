// Tests intentionally hold a std::sync::Mutex across .await to serialize
// against shared on-disk fixtures and mutated env vars.
#![allow(clippy::await_holding_lock)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::{LazyLock, Mutex};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn set_env(key: &str, value: &str) {
    unsafe {
        std::env::set_var(key, value);
    }
}

fn remove_env(key: &str) {
    unsafe {
        std::env::remove_var(key);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn git_passthrough_runs_real_git_once() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let log_path = temp.path().join("invocations.log");
    let git_path = temp.path().join("fake-git.sh");
    let script = format!(
        "#!/usr/bin/env sh\nprintf '%s\\n' \"$*\" >> \"{}\"\nif [ \"$1\" = \"status\" ]; then\n  exit 7\nfi\nexit 0\n",
        log_path.display()
    );
    fs::write(&git_path, script).unwrap();
    let mut perms = fs::metadata(&git_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&git_path, perms).unwrap();

    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();
    set_env("JERYU_SYSTEM_GIT", git_path.to_str().unwrap());
    remove_env("JERYU_GIT_MODE");

    let db = jeryu::state::Db::open_memory().await.unwrap();
    let exit = jeryu::git::executor::execute_git(Some(&db), &["status".into()])
        .await
        .unwrap();

    std::env::set_current_dir(cwd).unwrap();
    remove_env("JERYU_SYSTEM_GIT");

    assert_eq!(exit, 7);
    let invocations = fs::read_to_string(&log_path).unwrap();
    assert_eq!(invocations.lines().count(), 1);
    assert_eq!(invocations.trim(), "status");
}
