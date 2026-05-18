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
fn test_render_compose() {
    let composed = render_compose("example-root-password");
    assert!(composed.contains("container_name: jeryu-redline"));
    assert!(composed.contains("redlinedb/redline:latest"));
    assert!(composed.contains("profiles:"));
    assert!(composed.contains("- redline"));
    assert!(composed.contains("REDLINE_DB: \"jeryu\""));
    assert!(composed.contains("127.0.0.1:15432:5432"));
    assert!(composed.contains("container_name: jeryu-vault"));
    assert!(composed.contains("hashicorp/vault"));
    assert!(composed.contains("GITLAB_ROOT_PASSWORD: \"example-root-password\""));
    assert!(composed.contains("gitlab_workhorse['api_ci_long_polling_duration']"));
    assert!(composed.contains("docker-compose.yml")); // Should have some identifying comment
    assert!(composed.contains("puma['worker_processes'] = 0"));
    assert!(composed.contains("puma['max_threads'] = 8"));
    assert!(composed.contains("sidekiq['concurrency'] = 8"));
    assert!(composed.contains("mem_limit: 8g"));
    assert!(composed.contains("mem_reservation: 4g"));
    assert!(composed.contains("redis['save'] = []"));
    assert!(composed.contains("max-size: \"50m\""));
}

#[test]
fn test_render_runner_config() {
    let docker_cfg = render_runner_config(
        "default",
        "manager-1",
        "http://gitlab.local",
        "example-runner-token",
        "docker",
        "/tmp/jeryu-cache/default",
        4,
        2,
    );
    assert!(docker_cfg.contains("name = \"jeryu-default\""));
    assert!(docker_cfg.contains("executor = \"docker\""));
    assert!(docker_cfg.contains("builds_dir = \"/builds/default-manager-1\""));
    assert!(docker_cfg.contains("limit = 4"));
    assert!(docker_cfg.contains("privileged = false"));
    assert!(docker_cfg.contains("pull_policy = \"if-not-present\""));
    assert!(docker_cfg.contains("JERYU_CARGO_CACHE=1"));
    assert!(docker_cfg.contains("JERYU_CARGO_CACHE_ROOT=/cache"));
    assert!(docker_cfg.contains("pre_build_script"));
    assert!(docker_cfg.contains("JERYU_SCCACHE_ENABLED=1"));
    assert!(!docker_cfg.contains("/usr/local/bin/sccache:/usr/local/bin/sccache:ro"));
    assert!(!docker_cfg.contains("find /cache -mindepth 1 -maxdepth 1 -exec rm -rf"));
    assert!(!docker_cfg.contains("executor = \"custom\""));
    let parsed = docker_cfg.parse::<toml::Value>().unwrap();
    let runners = parsed
        .get("runners")
        .and_then(|value| value.as_array())
        .unwrap();
    let docker_runner = &runners[0];
    assert!(docker_runner.get("pre_build_script").is_some());
    let pre_build_script = docker_runner
        .get("pre_build_script")
        .and_then(toml::Value::as_str)
        .unwrap();
    assert!(!pre_build_script.contains("exit 0"));
    assert!(
        docker_runner
            .get("docker")
            .and_then(|value| value.get("pre_build_script"))
            .is_none()
    );

    let build_cfg = render_runner_config(
        "build",
        "manager-2",
        "http://gitlab.local",
        "example-runner-token",
        "docker",
        "/tmp/jeryu-cache/build",
        4,
        2,
    );
    assert!(build_cfg.contains("executor = \"docker\""));
    assert!(build_cfg.contains("privileged = true"));

    let custom_cfg = render_runner_config(
        "default",
        "manager/with spaces",
        "http://gitlab.local",
        "example-runner-token",
        "custom",
        "/tmp/jeryu-cache/default",
        4,
        2,
    );
    assert!(custom_cfg.contains("executor = \"custom\""));
    assert!(custom_cfg.contains("builds_dir = \"/builds/default-manager-with-spaces\""));
    assert!(custom_cfg.contains("config_args = [\"exec\", \"config\"]"));
    assert!(custom_cfg.contains("run_args = [\"exec\", \"run\"]"));
    assert!(custom_cfg.contains("JERYU_CARGO_CACHE_ROOT=/pool-cache"));
    assert!(!custom_cfg.contains("pre_build_script ="));
}

#[test]
fn manager_builds_dir_is_pool_and_manager_scoped() {
    assert_eq!(
        manager_builds_dir("build/pool", "manager 123"),
        "/builds/build-pool-manager-123"
    );
    assert_ne!(
        manager_builds_dir("build", "manager-a"),
        manager_builds_dir("build", "manager-b")
    );
}

#[test]
fn runner_shutdown_timeout_uses_env_override() {
    let _guard = ENV_LOCK.lock().unwrap();
    let original = std::env::var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS").ok();
    set_env_var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS", "12");

    assert_eq!(runner_shutdown_timeout_secs(), 12);

    match original {
        Some(value) => set_env_var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS", value),
        None => remove_env_var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS"),
    }
}

#[test]
fn legacy_redline_service_url_uses_embedded_file_url() {
    let _guard = ENV_LOCK.lock().unwrap();
    let original = std::env::var("JERYU_DATABASE_URL").ok();
    set_env_var(
        "JERYU_DATABASE_URL",
        "redline://jeryu:secret@127.0.0.1:15432/jeryu",
    );

    let url = database_url().expect("database url");
    assert!(url.starts_with("redline:"));
    assert!(!url.starts_with("redline://"));
    assert!(url.contains("jeryu.db"));

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

    assert_eq!(database_url().as_deref(), Some("redline::memory:"));

    match original {
        Some(value) => set_env_var("JERYU_DATABASE_URL", value),
        None => remove_env_var("JERYU_DATABASE_URL"),
    }
}
