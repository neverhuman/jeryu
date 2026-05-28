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
    for tool in [
        "awk",
        "cat",
        "cut",
        "date",
        "mkdir",
        "rm",
        "rmdir",
        "sha256sum",
    ] {
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

fn make_rust_only_bin_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    let resolve = |name: &str| -> String {
        let output = std::process::Command::new("which")
            .arg(name)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    std::os::unix::fs::symlink(resolve("cargo"), dir.path().join("cargo")).unwrap();
    std::os::unix::fs::symlink(resolve("rustc"), dir.path().join("rustc")).unwrap();
    dir
}

fn count_lease_files(root: &std::path::Path) -> usize {
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                == Some(LEASES_DIR_NAME)
            {
                count += 1;
            }
        }
    }
    count
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
    assert!(layout.env.contains_key("CARGO_HOME"));
    assert!(layout.env.contains_key("RUSTUP_HOME"));
    assert_eq!(layout.env["JERYU_CARGO_TARGET_PROFILE"], "debug");
    assert!(layout.env.contains_key("SCCACHE_DIR"));
    assert!(layout.env.contains_key("SCCACHE_NO_DAEMON"));
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
    assert!(layout.env.contains_key("CARGO_HOME"));
    assert!(layout.env.contains_key("RUSTUP_HOME"));
    assert_eq!(layout.env["JERYU_CARGO_TARGET_PROFILE"], "debug");
    assert_eq!(layout.env["SCCACHE_NO_DAEMON"], "1");
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
        .arg("-c")
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
fn runner_pre_build_script_exports_cargo_and_rustup_homes() {
    let pool_cache = TempDir::new().unwrap();
    let script =
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker");
    assert!(script.contains("export CARGO_HOME="));
    assert!(script.contains("export RUSTUP_HOME="));
    assert!(script.contains(".jeryu-cache-stamp.json"));
    assert!(script.contains(".jeryu-cache-seeds"));
    assert!(script.contains(".jeryu-cache-promotions"));
}

#[test]
fn runner_pre_build_script_marks_active_lease_and_uses_pool_sccache() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(true, true, false);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let pool_cache = TempDir::new().unwrap();
    let tools_dir = pool_cache.path().join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    let sccache_bin = tools_dir.join("sccache");
    std::fs::write(
        &sccache_bin,
        "#!/bin/sh\ncase \"$1\" in --start-server|--stop-server|--show-stats) exit 0;; esac\nexec \"$@\"\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&sccache_bin).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&sccache_bin, perms).unwrap();

    let script = format!(
        "{}\nset -- \"$CARGO_TARGET_DIR\"/{LEASES_DIR_NAME}/*.json\nif [ -s \"$1\" ]; then lease_state=present; else lease_state=missing; fi\nprintf '%s\\n' \"$CARGO_TARGET_DIR|$RUSTC_WRAPPER|$SCCACHE_DIR|$lease_state|$1\"\n",
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker",)
    );
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .env("JERYU_CARGO_CACHE", "1")
        .env("JERYU_SCCACHE_ENABLED", "1")
        .env("CI_PROJECT_PATH_SLUG", "demo-project")
        .env("CI_JOB_ID", "4242")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    assert!(line.contains("/cargo-targets/demo-project/"));
    assert!(line.contains("|sccache|"));
    assert!(line.contains("/sccache|present|"));
    assert!(line.contains("/.jeryu-leases/4242-"));
    assert_eq!(count_lease_files(pool_cache.path()), 0);

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn runner_pre_build_script_prestarts_sccache_without_no_daemon() {
    let pool_cache = TempDir::new().unwrap();
    let script =
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker");
    assert!(script.contains("sccache --start-server >/dev/null 2>&1 || true"));
    assert!(!script.contains("export SCCACHE_NO_DAEMON"));
}

#[test]
fn runner_pre_build_script_can_isolate_target_by_runner_slot() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(true, true, false);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let pool_cache = TempDir::new().unwrap();
    let script = format!(
        "{}\nprintf '%s\\n' \"$CARGO_TARGET_DIR\"\n",
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker",)
    );
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .env("JERYU_CARGO_CACHE", "1")
        .env("JERYU_SCCACHE_ENABLED", "0")
        .env("JERYU_CARGO_TARGET_ISOLATE", "slot")
        .env("CI_PROJECT_PATH_SLUG", "demo-project")
        .env("CI_CONCURRENT_ID", "7")
        .env("CI_RUNNER_ID", "runner-1")
        .env_remove("CI_BUILDS_DIR")
        .env_remove("CI_RUNNER_SHORT_TOKEN")
        .env_remove("CI_CONCURRENT_PROJECT_ID")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let target_dir = stdout.trim();
    assert!(target_dir.contains("/cargo-targets/demo-project/"));
    assert!(target_dir.contains("/slots/runner-1-7/target"));

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn runner_pre_build_script_uses_manager_key_for_slot_isolation() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(true, true, false);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let pool_cache = TempDir::new().unwrap();
    let script = format!(
        "{}\nprintf '%s\\n' \"$CARGO_TARGET_DIR\"\n",
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker",)
    );
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .env("JERYU_CARGO_CACHE", "1")
        .env("JERYU_SCCACHE_ENABLED", "0")
        .env("JERYU_CARGO_TARGET_ISOLATE", "slot")
        .env("CI_PROJECT_PATH_SLUG", "demo-project")
        .env("CI_BUILDS_DIR", "/builds/build-abc123")
        .env("CI_CONCURRENT_ID", "0")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let target_dir = stdout.trim();
    assert!(target_dir.contains("/cargo-targets/demo-project/"));
    assert!(target_dir.contains("/slots/build-abc123-0/target"));

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn runner_pre_build_script_caps_cargo_build_jobs_by_host_and_runner_slots() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_test_bin_dir(true, true, false);
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let pool_cache = TempDir::new().unwrap();
    let script = format!(
        "{}\nprintf '%s\\n' \"$CARGO_BUILD_JOBS|$JERYU_CARGO_RESERVED_CORES|$JERYU_CARGO_TOTAL_SLOTS|$JERYU_CARGO_AUTO_BUILD_JOBS\"\n",
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker",)
    );
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .env("JERYU_CARGO_CACHE", "1")
        .env("JERYU_SCCACHE_ENABLED", "0")
        .env("JERYU_CARGO_HOST_CORES", "128")
        .env("JERYU_CARGO_TOTAL_RUNNER_SLOTS", "20")
        .env("CARGO_BUILD_JOBS", "8")
        .env("CI_PROJECT_PATH_SLUG", "demo-project")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "4|32|20|4");

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
}

#[test]
fn runner_pre_build_script_missing_helper_tools_does_not_short_circuit_job() {
    let _guard = ENV_LOCK.lock().unwrap();
    let path_dir = make_rust_only_bin_dir();
    let original_path = std::env::var_os("PATH");
    set_env_var("PATH", path_dir.path());
    let pool_cache = TempDir::new().unwrap();
    let script = format!(
        "{}\nprintf '%s\\n' user-script-ran\nexit 19\n",
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

    assert_eq!(output.status.code(), Some(19));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "user-script-ran"
    );
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

/// Regression guard: `rustc -vV` must appear in the generated script BEFORE
/// `export RUSTUP_HOME`.  Rust official containers use the rustup dispatch binary
/// as `rustc`; it reads `RUSTUP_HOME` to locate the toolchain.  If we override
/// `RUSTUP_HOME` first (to an empty cache directory), the dispatch binary cannot
/// find the toolchain and aborts with "no default toolchain configured".
#[test]
fn runner_pre_build_script_probes_rustc_before_overriding_rustup_home() {
    let pool_cache = TempDir::new().unwrap();
    let script =
        render_runner_cargo_pre_build_script(&pool_cache.path().display().to_string(), "docker");

    let rustc_probe_pos = script
        .find("rustc -vV")
        .expect("script must contain rustc -vV probe");
    let rustup_export_pos = script
        .find("export RUSTUP_HOME=")
        .expect("script must contain export RUSTUP_HOME=");

    assert!(
        rustc_probe_pos < rustup_export_pos,
        "rustc -vV probe (pos {rustc_probe_pos}) must appear before \
         export RUSTUP_HOME (pos {rustup_export_pos}); \
         the rustup dispatch binary reads RUSTUP_HOME to find the toolchain so the probe \
         must run before RUSTUP_HOME is redirected to the empty cache directory"
    );
}
