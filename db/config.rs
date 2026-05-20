//! RedlineDB configuration surface for the state-store boundary.

use std::path::{Path, PathBuf};

/// Where the embedded RedlineDB state file lives.
pub fn state_path() -> PathBuf {
    crate::config::data_dir().join("jeryu.db")
}

/// SQLx URL for an embedded RedlineDB file path.
pub fn embedded_redline_url(path: &Path) -> String {
    format!("redline://{}", path.display())
}

/// Optional RedlineDB URL override.
pub fn configured_url() -> Option<String> {
    match std::env::var("JERYU_DATABASE_URL").ok() {
        Some(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
        // SAFETY: this test module serializes environment mutation with ENV_LOCK
        // and restores prior values before releasing the lock.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
        // SAFETY: this test module serializes environment mutation with ENV_LOCK
        // and restores prior values before releasing the lock.
        unsafe {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn configured_url_preserves_service_urls_for_fail_closed_backend_validation() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("JERYU_DATABASE_URL").ok();
        set_env_var(
            "JERYU_DATABASE_URL",
            "redline://jeryu:secret@127.0.0.1:15432/jeryu",
        );

        let url = configured_url().expect("database url");
        assert_eq!(url, "redline://jeryu:secret@127.0.0.1:15432/jeryu");

        match original {
            Some(value) => set_env_var("JERYU_DATABASE_URL", value),
            None => remove_env_var("JERYU_DATABASE_URL"),
        }
    }

    #[test]
    fn embedded_redline_memory_url_is_preserved() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("JERYU_DATABASE_URL").ok();
        set_env_var("JERYU_DATABASE_URL", "redline::memory:");

        assert_eq!(configured_url().as_deref(), Some("redline::memory:"));

        match original {
            Some(value) => set_env_var("JERYU_DATABASE_URL", value),
            None => remove_env_var("JERYU_DATABASE_URL"),
        }
    }

    #[test]
    fn embedded_redline_file_urls_are_preserved() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("JERYU_DATABASE_URL").ok();

        set_env_var(
            "JERYU_DATABASE_URL",
            "redline:///tmp/jeryu/target/jeryu/autonomy.redlineDB",
        );
        assert_eq!(
            configured_url().as_deref(),
            Some("redline:///tmp/jeryu/target/jeryu/autonomy.redlineDB")
        );

        set_env_var(
            "JERYU_DATABASE_URL",
            "redlineDB:///tmp/jeryu/target/jeryu/autonomy.redlineDB",
        );
        assert_eq!(
            configured_url().as_deref(),
            Some("redlineDB:///tmp/jeryu/target/jeryu/autonomy.redlineDB")
        );

        match original {
            Some(value) => set_env_var("JERYU_DATABASE_URL", value),
            None => remove_env_var("JERYU_DATABASE_URL"),
        }
    }
}
