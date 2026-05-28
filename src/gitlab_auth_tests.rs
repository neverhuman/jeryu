use super::*;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

struct EnvGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    fn new(home: &Path) -> Self {
        let keys = [
            "HOME",
            "GITLAB_PAT",
            "GITLAB_TOKEN",
            "PRIVATE_TOKEN",
            "GITLAB_URL",
            "CI_SERVER_URL",
        ];
        let saved = keys
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect::<Vec<_>>();
        // SAFETY: these tests serialize process-env mutation with a
        // global mutex and restore every touched key in EnvGuard::drop.
        unsafe {
            std::env::set_var("HOME", home);
            for key in TOKEN_KEYS {
                std::env::remove_var(key);
            }
            std::env::remove_var("GITLAB_URL");
            std::env::remove_var("CI_SERVER_URL");
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: restoration runs while the test still holds the same
        // global mutex that guarded the corresponding mutation.
        unsafe {
            for (key, value) in self.saved.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

#[test]
fn loads_gitlab_pat_from_canonical_env_file() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::new(temp.path());
    let env_path = config::env_file();
    std::fs::create_dir_all(env_path.parent().unwrap()).unwrap();
    std::fs::write(
        &env_path,
        "JERYU_WEBHOOK_SECRET=kept\nGITLAB_PAT=from-file\n",
    )
    .unwrap();

    let token = load_token_for_url("http://127.0.0.1:8929")
        .unwrap()
        .unwrap();

    assert_eq!(token, "from-file");
}

#[test]
fn local_gitlab_url_classifier_accepts_gitlab_local() {
    assert!(is_local_gitlab_url("http://gitlab.local:8929"));
    assert!(is_local_gitlab_url("http://localhost:8929"));
    assert!(!is_local_gitlab_url("https://gitlab.example.invalid"));
}

#[test]
fn upserts_gitlab_pat_and_preserves_other_keys() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::new(temp.path());
    let env_path = config::env_file();
    std::fs::create_dir_all(env_path.parent().unwrap()).unwrap();
    std::fs::write(
        &env_path,
        "GITLAB_ROOT_PASSWORD=root\nGITLAB_PAT=old\nRUNNER_TOKEN_DEFAULT=runner\nJERYU_DATABASE_URL=redlinedb://local\n",
    )
    .unwrap();

    upsert_pat("new-value").unwrap();
    upsert_pat("new-value").unwrap();

    let text = std::fs::read_to_string(&env_path).unwrap();
    assert_eq!(text.matches("GITLAB_PAT=").count(), 1);
    assert!(text.contains("GITLAB_ROOT_PASSWORD=root"));
    assert!(text.contains("RUNNER_TOKEN_DEFAULT=runner"));
    assert!(text.contains("JERYU_DATABASE_URL=redlinedb://local"));
    assert!(text.contains("GITLAB_PAT=new-value"));
}

#[cfg(unix)]
#[test]
fn env_file_permissions_are_normalized_to_0600() {
    use std::os::unix::fs::PermissionsExt;

    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::new(temp.path());
    let env_path = config::env_file();
    std::fs::create_dir_all(env_path.parent().unwrap()).unwrap();
    std::fs::write(&env_path, "GITLAB_PAT=from-file\n").unwrap();
    let mut perms = std::fs::metadata(&env_path).unwrap().permissions();
    perms.set_mode(0o644);
    std::fs::set_permissions(&env_path, perms).unwrap();

    ensure_env_file_permissions().unwrap();

    let mode = std::fs::metadata(&env_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn missing_local_pat_is_repaired_and_stored() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::new(temp.path());
    let env_path = config::env_file();
    std::fs::create_dir_all(env_path.parent().unwrap()).unwrap();
    std::fs::write(&env_path, "JERYU_WEBHOOK_SECRET=kept\n").unwrap();

    let auth = resolve_or_repair_with("http://localhost:8929", |_url| async {
        Ok("generated-value".to_string())
    })
    .await
    .unwrap();

    assert_eq!(auth.token, "generated-value");
    let text = std::fs::read_to_string(&env_path).unwrap();
    assert!(text.contains("JERYU_WEBHOOK_SECRET=kept"));
    assert!(text.contains("GITLAB_PAT=generated-value"));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn non_local_url_does_not_repair() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::new(temp.path());
    let env_path = config::env_file();
    std::fs::create_dir_all(env_path.parent().unwrap()).unwrap();
    std::fs::write(&env_path, "GITLAB_PAT=local-only\n").unwrap();
    let mut repaired = false;

    let result = resolve_or_repair_with("https://gitlab.example.invalid", |_url| {
        repaired = true;
        async { Ok("unused".to_string()) }
    })
    .await;

    assert!(result.is_err());
    assert!(!repaired);
}
