use super::*;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

// Use the crate-wide PATH_ENV_LOCK so we serialize against EVERY test that
// touches PATH (sandbox::tests::test_sandbox_proxy_injection,
// remote_shell_tests::*, etc.), not just other tests inside this file.
use crate::test_sync::PATH_ENV_LOCK as ENV_LOCK;

fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
    // SAFETY: these tests serialize environment mutation with ENV_LOCK and
    // restore previous values before releasing the lock.
    unsafe {
        std::env::set_var(key, value);
    }
}

fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
    // SAFETY: these tests serialize environment mutation with ENV_LOCK and
    // restore previous values before releasing the lock.
    unsafe {
        std::env::remove_var(key);
    }
}

fn make_test_bin_dir(include_cargo: bool, include_rustc: bool, include_sccache: bool) -> TempDir {
    let dir = TempDir::new().unwrap();
    let resolve = |name: &str| -> String {
        let output = std::process::Command::new("which")
            .arg(name)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    let cargo_path = resolve("cargo");
    let rustc_path = resolve("rustc");

    if include_cargo {
        std::os::unix::fs::symlink(&cargo_path, dir.path().join("cargo")).unwrap();
    }
    if include_rustc {
        std::os::unix::fs::symlink(&rustc_path, dir.path().join("rustc")).unwrap();
    }
    for tool in ["awk", "cut", "mkdir", "sha256sum"] {
        let tool_path = resolve(tool);
        if !tool_path.is_empty() {
            std::os::unix::fs::symlink(tool_path, dir.path().join(tool)).unwrap();
        }
    }
    if include_sccache {
        std::fs::write(dir.path().join("sccache"), "#!/bin/sh\nexec \"$@\"\n").unwrap();
        let mut perms = std::fs::metadata(dir.path().join("sccache"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(dir.path().join("sccache"), perms).unwrap();
    }
    dir
}

#[test]
fn repo_key_is_deterministic() {
    let dir = TempDir::new().unwrap();
    let key1 = canonical_repo_key(dir.path()).unwrap();
    let key2 = canonical_repo_key(dir.path()).unwrap();
    assert_eq!(key1, key2);
    assert_eq!(key1.len(), 12);
}

#[test]
fn layout_uses_expected_segments() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(false, true, false);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let cache_root = std::path::PathBuf::from("/tmp/jeryu-cache");
    let layout = build_cargo_cache_layout(
        &cache_root,
        "targets",
        "repo-key",
        true,
        Some("job-123"),
        Some("1"),
    )
    .unwrap();

    assert!(layout.target_dir.ends_with("target"));
    assert!(layout.target_root.to_string_lossy().contains("job-123"));
    assert!(layout.env.contains_key("CARGO_TARGET_DIR"));
    assert_eq!(layout.env["CARGO_INCREMENTAL"], "1");
    assert!(!layout.env.contains_key("RUSTC_WRAPPER"));

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn layout_defaults_incremental_to_zero() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(false, true, false);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());

    let layout = build_cargo_cache_layout(
        std::path::Path::new("/tmp/jeryu-cache"),
        "targets",
        "repo-key",
        true,
        None,
        None,
    )
    .unwrap();

    assert_eq!(layout.env["CARGO_INCREMENTAL"], "0");
    assert!(layout.incremental_override.is_none());

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn shell_exports_quote_values() {
    let layout = CargoCacheLayout {
        scope_key: "scope".to_string(),
        cache_root: std::path::PathBuf::from("/tmp/root"),
        target_root: std::path::PathBuf::from("/tmp/root/targets"),
        target_dir: std::path::PathBuf::from("/tmp/root/targets/scope/target"),
        sccache_dir: std::path::PathBuf::from("/tmp/root/sccache"),
        toolchain: CargoToolchainKey {
            rustc_key: "abc".to_string(),
            rustc_version: "rustc 1.0.0".to_string(),
            host_triple: "x86_64-unknown-linux-gnu".to_string(),
        },
        cargo_cache_enabled: true,
        incremental_override: None,
        env: std::collections::BTreeMap::from([("A".to_string(), "b c'd".to_string())]),
        lease_dir: None,
    };
    let lines = shell_exports(&layout);
    assert_eq!(
        lines,
        vec![
            "export A='b c'\\''d'".to_string(),
            "unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_NO_DAEMON SCCACHE_CACHE_SIZE".to_string()
        ]
    );
}

#[test]
fn layout_adds_sccache_when_usable() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(false, true, true);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let layout = build_cargo_cache_layout(
        std::path::Path::new("/tmp/jeryu-cache"),
        "targets",
        "repo-key",
        true,
        None,
        None,
    )
    .unwrap();
    assert!(layout.env.contains_key("RUSTC_WRAPPER"));
    assert!(layout.env.contains_key("SCCACHE_DIR"));
    assert_eq!(layout.env["SCCACHE_CACHE_SIZE"], "10G");
    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn concurrent_leases_do_not_remove_each_other() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(false, true, false);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let dir = TempDir::new().unwrap();
    let layout =
        build_cargo_cache_layout(dir.path(), "targets", "repo-key", true, None, None).unwrap();

    let first = write_lease(&layout).unwrap().unwrap();
    let second = write_lease(&layout).unwrap().unwrap();
    let lease_dir = layout.lease_dir.clone().unwrap();
    let lease_count = std::fs::read_dir(&lease_dir).unwrap().count();
    assert_eq!(lease_count, 2);

    drop(first);
    assert_eq!(std::fs::read_dir(&lease_dir).unwrap().count(), 1);
    let scan = scan_target_leases(&layout.target_dir);
    assert!(scan.active);

    drop(second);
    let scan = scan_target_leases(&layout.target_dir);
    assert!(!scan.active);
    assert_eq!(scan.observed_files, 0);

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn scan_target_leases_cleans_stale_files_but_keeps_active_lease() {
    let dir = TempDir::new().unwrap();
    let lease_dir = dir.path().join("target").join(LEASES_DIR_NAME);
    std::fs::create_dir_all(&lease_dir).unwrap();
    let expired = CargoLeaseRecord {
        kind: "local-cargo".to_string(),
        scope_key: "scope".to_string(),
        target_dir: dir.path().display().to_string(),
        pid: u32::MAX,
        created_at: chrono::Utc::now().to_rfc3339(),
        rustc_key: "key".to_string(),
        rustc_version: "rustc".to_string(),
        host_triple: "host".to_string(),
    };
    let active = CargoLeaseRecord {
        pid: std::process::id(),
        ..expired.clone()
    };
    std::fs::write(
        lease_dir.join("expired-a.json"),
        serde_json::to_vec_pretty(&expired).unwrap(),
    )
    .unwrap();
    std::fs::write(
        lease_dir.join("expired-b.json"),
        serde_json::to_vec_pretty(&expired).unwrap(),
    )
    .unwrap();
    std::fs::write(
        lease_dir.join("active.json"),
        serde_json::to_vec_pretty(&active).unwrap(),
    )
    .unwrap();

    let scan = scan_target_leases(&dir.path().join("target"));
    assert!(scan.active);
    assert_eq!(scan.observed_files, 3);
    assert_eq!(scan.stale_files, 2);
    assert!(lease_dir.join("active.json").exists());
    assert!(!lease_dir.join("expired-a.json").exists());
}

#[test]
fn runner_pre_build_script_sets_target_dir_without_sccache() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(true, true, false);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let pool_cache = TempDir::new().unwrap();
    let script = format!(
        "{}\nprintf '%s\\n' \"$CARGO_TARGET_DIR|${{RUSTC_WRAPPER-}}\"\n",
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker",)
    );
    let output = std::process::Command::new("/bin/sh")
        .arg("-lc")
        .arg(script)
        .env("JERYU_CARGO_CACHE", "1")
        .env("JERYU_SCCACHE_ENABLED", "1")
        .env("CI_PROJECT_PATH_SLUG", "demo-project")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    assert!(line.contains("/cargo-targets/demo-project/"));
    assert!(line.ends_with('|'));
    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn runner_pre_build_script_missing_rust_tools_does_not_short_circuit_job() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = TempDir::new().unwrap();
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let pool_cache = TempDir::new().unwrap();
    let script = format!(
        "{}\nprintf '%s\\n' user-script-ran\nexit 17\n",
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker",)
    );
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .env("JERYU_CARGO_CACHE", "1")
        .output()
        .unwrap();

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }

    assert_eq!(output.status.code(), Some(17));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "user-script-ran"
    );
}
