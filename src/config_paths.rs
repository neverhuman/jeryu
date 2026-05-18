use super::*;

fn data_root() -> PathBuf {
    dirs_home().join(".jeryu")
}

fn workspace_path(parts: &[&str]) -> PathBuf {
    parts.iter().fold(data_root(), |path, part| path.join(part))
}

fn cache_path(parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(cache_root_dir(), |path, part| path.join(part))
}

/// Root data directory for jeryu state on the host.
///
/// Defaults to `~/.jeryu` so a fresh bootstrap creates the canonical path.
pub fn data_dir() -> PathBuf {
    data_root()
}

/// Where jeryu.env lives (secrets file).
pub fn env_file() -> PathBuf {
    workspace_path(&["jeryu.env"])
}

/// Where the RedlineDB database lives.
pub fn db_path() -> PathBuf {
    workspace_path(&["jeryu.db"])
}

/// Persistent data root for the jeryu RedlineDB service.
pub fn redline_data_dir() -> PathBuf {
    workspace_path(&["redline"])
}

/// Database URL override used for RedlineDB or explicit compatibility paths.
pub fn database_url() -> Option<String> {
    match std::env::var("JERYU_DATABASE_URL").ok() {
        Some(value) if !value.trim().is_empty() => {
            let value = value.trim();
            if is_legacy_redline_service_url(value) {
                Some(format!("redline:{}?mode=rwc", db_path().display()))
            } else {
                Some(value.to_string())
            }
        }
        _ => None,
    }
}

fn is_legacy_redline_service_url(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    (value.starts_with("redline://") || value.starts_with("redlineql://"))
        && (value.contains("@127.0.0.1")
            || value.contains("@localhost")
            || value.contains("://127.0.0.1")
            || value.contains("://localhost"))
}

/// Where runner config directories are created (one per manager).
pub fn runners_dir() -> PathBuf {
    workspace_path(&["runners"])
}

/// Root directory for runner cache bind mounts.
pub fn cache_root_dir() -> PathBuf {
    workspace_path(&["cache"])
}

/// Dedicated cache directory for a single runner manager.
pub fn manager_cache_dir(manager_id: &str) -> PathBuf {
    cache_path(&["managers", manager_id])
}

/// Root for local agent-owned Cargo caches.
pub fn local_cargo_cache_root() -> PathBuf {
    cache_path(&["local-cargo"])
}

/// Local agent Cargo target cache root.
pub fn local_cargo_targets_root() -> PathBuf {
    cache_path(&["local-cargo", "targets"])
}

/// Local agent Cargo sccache root.
pub fn local_cargo_sccache_dir() -> PathBuf {
    cache_path(&["local-cargo", "sccache"])
}

/// Root for a pool-scoped runner cache namespace.
pub fn pool_cache_root(pool_name: &str) -> PathBuf {
    cache_path(&["pools", pool_name])
}

/// Pool-scoped Cargo target cache root.
pub fn pool_cargo_targets_root(pool_name: &str) -> PathBuf {
    cache_path(&["pools", pool_name, "cargo-targets"])
}

/// Pool-scoped sccache root.
pub fn pool_cargo_sccache_dir(pool_name: &str) -> PathBuf {
    cache_path(&["pools", pool_name, "sccache"])
}

/// Inside-container mount path for the shared pool cache.
pub fn pool_cache_mount_path(executor: &str) -> &'static str {
    if executor == "custom" {
        "/pool-cache"
    } else {
        "/cache"
    }
}

fn safe_builds_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

/// Manager-local GitLab Runner builds directory.
///
/// GitLab Runner may process multiple jobs with the same job name across
/// concurrent pipelines. Keeping the runner root manager-unique prevents those
/// jobs from sharing checkout state when a project also sets GIT_CLONE_PATH.
pub fn manager_builds_dir(pool_name: &str, manager_id: &str) -> String {
    format!(
        "/builds/{}-{}",
        safe_builds_component(pool_name),
        safe_builds_component(manager_id)
    )
}

/// Timeout, in seconds, used when waiting for runner managers to exit after SIGQUIT.
/// The production default comes from settings, but CI/test runs use a shorter default path
/// unless an explicit override is provided.
pub fn runner_shutdown_timeout_secs() -> u64 {
    if let Some(value) = std::env::var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        return value;
    }

    if is_test_or_ci_runtime() {
        return 30;
    }

    crate::settings::get().pool.runner_shutdown_timeout_secs
}

/// Where docker-compose.yml is written.
fn dirs_home() -> PathBuf {
    dirs::home_dir().expect("cannot determine home directory")
}

fn is_test_or_ci_runtime() -> bool {
    std::env::var_os("CI").is_some()
        || std::env::var_os("GITHUB_ACTIONS").is_some()
        || std::env::var_os("GITLAB_CI").is_some()
        || std::env::var_os("RUST_TEST_THREADS").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_is_under_jeryu() {
        assert!(data_dir().ends_with(".jeryu"));
    }

    #[test]
    fn cache_roots_are_nested_under_cache() {
        assert!(cache_root_dir().ends_with("cache"));
        assert!(local_cargo_cache_root().ends_with("local-cargo"));
        assert!(pool_cache_root("build").ends_with("pools/build"));
    }
}
