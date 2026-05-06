//! Owner: Cargo cache layout and local agent helpers
//! Proof: `cargo test -p jeryu -- cargo_cache`
//! Invariants: Cache keys are deterministic; target dirs stay scoped by repo/project, toolchain, and host triple; active leases are never collected.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const LEASES_DIR_NAME: &str = ".jeryu-leases";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoToolchainKey {
    pub rustc_key: String,
    pub rustc_version: String,
    pub host_triple: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoCacheLayout {
    pub scope_key: String,
    pub cache_root: PathBuf,
    pub target_root: PathBuf,
    pub target_dir: PathBuf,
    pub sccache_dir: PathBuf,
    pub toolchain: CargoToolchainKey,
    pub cargo_cache_enabled: bool,
    pub incremental_override: Option<String>,
    pub env: BTreeMap<String, String>,
    pub lease_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLeaseRecord {
    pub kind: String,
    pub scope_key: String,
    pub target_dir: String,
    pub pid: u32,
    pub created_at: String,
    pub rustc_key: String,
    pub rustc_version: String,
    pub host_triple: String,
}

pub struct CargoLeaseGuard {
    path: PathBuf,
    parent_dir: PathBuf,
}

impl Drop for CargoLeaseGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.parent_dir);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoLeaseScan {
    pub active: bool,
    pub observed_files: usize,
    pub stale_files: usize,
}

pub fn canonical_repo_key(repo_root: &Path) -> Result<String> {
    let canonical = repo_root
        .canonicalize()
        .with_context(|| format!("canonicalize repo root {}", repo_root.display()))?;
    Ok(short_hash(canonical.to_string_lossy().as_bytes()))
}

pub fn build_cargo_cache_layout(
    cache_root: &Path,
    target_root_name: &str,
    scope_key: &str,
    cache_enabled: bool,
    isolate_job_key: Option<&str>,
    incremental_override: Option<&str>,
) -> Result<CargoCacheLayout> {
    let toolchain = current_rustc_toolchain()?;
    let scope_key = sanitize_segment(scope_key);
    let mut target_root = cache_root
        .join(target_root_name)
        .join(&scope_key)
        .join(&toolchain.rustc_key)
        .join(&toolchain.host_triple);

    if let Some(job_key) = isolate_job_key.filter(|value| !value.trim().is_empty()) {
        target_root = target_root.join("jobs").join(sanitize_segment(job_key));
    }

    let target_dir = target_root.join("target");
    let sccache_dir = cache_root.join("sccache");
    let incremental_override = incremental_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let cargo_incremental: String = match incremental_override.as_deref() {
        Some(value) => value.to_string(),
        None => "0".to_string(),
    };

    let mut env = BTreeMap::new();
    env.insert(
        "JERYU_CARGO_CACHE".to_string(),
        if cache_enabled { "1" } else { "0" }.to_string(),
    );
    env.insert(
        "JERYU_CARGO_CACHE_ROOT".to_string(),
        cache_root.display().to_string(),
    );
    env.insert("JERYU_CARGO_SCOPE_KEY".to_string(), scope_key.clone());
    env.insert(
        "JERYU_CARGO_RUSTC_KEY".to_string(),
        toolchain.rustc_key.clone(),
    );
    env.insert(
        "JERYU_CARGO_RUSTC_VERSION".to_string(),
        toolchain.rustc_version.clone(),
    );
    env.insert(
        "JERYU_CARGO_HOST_TRIPLE".to_string(),
        toolchain.host_triple.clone(),
    );
    env.insert("CARGO_INCREMENTAL".to_string(), cargo_incremental.clone());

    if cache_enabled {
        env.insert(
            "CARGO_TARGET_DIR".to_string(),
            target_dir.display().to_string(),
        );
    }

    if let Some(sccache_binary) = usable_sccache_binary() {
        env.insert(
            "RUSTC_WRAPPER".to_string(),
            sccache_binary.display().to_string(),
        );
        env.insert("SCCACHE_DIR".to_string(), sccache_dir.display().to_string());
        env.insert("SCCACHE_NO_DAEMON".to_string(), "1".to_string());
        env.insert(
            "SCCACHE_CACHE_SIZE".to_string(),
            crate::settings::get().sccache.cache_size.clone(),
        );
    }

    let lease_dir = if cache_enabled {
        Some(target_dir.join(LEASES_DIR_NAME))
    } else {
        None
    };

    Ok(CargoCacheLayout {
        scope_key,
        cache_root: cache_root.to_path_buf(),
        target_root,
        target_dir,
        sccache_dir,
        toolchain,
        cargo_cache_enabled: cache_enabled,
        incremental_override,
        env,
        lease_dir,
    })
}

pub fn local_cargo_layout(repo_root: &Path, cache_enabled: bool) -> Result<CargoCacheLayout> {
    let incremental_override = std::env::var("JERYU_CARGO_INCREMENTAL").ok();
    build_cargo_cache_layout(
        &crate::config::local_cargo_cache_root(),
        "targets",
        &canonical_repo_key(repo_root)?,
        cache_enabled,
        None,
        incremental_override.as_deref(),
    )
}

pub fn runner_cargo_layout(
    cache_root: &Path,
    scope_key: &str,
    cache_enabled: bool,
    isolate_job_key: Option<&str>,
    incremental_override: Option<&str>,
) -> Result<CargoCacheLayout> {
    build_cargo_cache_layout(
        cache_root,
        "cargo-targets",
        scope_key,
        cache_enabled,
        isolate_job_key,
        incremental_override,
    )
}

pub fn render_runner_cargo_pre_build_script(pool_cache_mount: &str, executor: &str) -> String {
    let _ = executor;
    format!(
        r#"set -eu
if [ "${{JERYU_CARGO_CACHE:-1}}" != "0" ] && command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
  RUSTC_INFO="$(rustc -vV)"
  HOST_TRIPLE="$(printf '%s\n' "$RUSTC_INFO" | awk '/^host: / {{ print $2; exit }}')"
  RUSTC_VERSION="$(printf '%s\n' "$RUSTC_INFO" | awk '/^release: / {{ print $2; exit }}')"
  if [ -n "$HOST_TRIPLE" ] && [ -n "$RUSTC_VERSION" ]; then
    RUSTC_KEY="$(printf '%s\n' "$RUSTC_INFO" | sha256sum | cut -c1-12)"
    JERYU_CARGO_SCOPE_KEY="${{CI_PROJECT_PATH_SLUG:-unknown-project}}"
    JERYU_CARGO_CACHE_ROOT="{pool_cache_mount}"
    JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_CACHE_ROOT/cargo-targets/$JERYU_CARGO_SCOPE_KEY/$RUSTC_KEY/$HOST_TRIPLE"
    if [ "${{JERYU_CARGO_TARGET_ISOLATE:-}}" = "job" ]; then
      JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_TARGET_ROOT/jobs/${{CI_JOB_ID:-unknown}}"
    fi
    export JERYU_CARGO_CACHE_ROOT JERYU_CARGO_SCOPE_KEY JERYU_CARGO_RUSTC_KEY="$RUSTC_KEY" JERYU_CARGO_RUSTC_VERSION="$RUSTC_VERSION" JERYU_CARGO_HOST_TRIPLE="$HOST_TRIPLE"
    export CARGO_TARGET_DIR="$JERYU_CARGO_TARGET_ROOT/target"
    mkdir -p "$CARGO_TARGET_DIR"
    if [ -n "${{JERYU_CARGO_INCREMENTAL:-}}" ]; then
      export CARGO_INCREMENTAL="$JERYU_CARGO_INCREMENTAL"
    else
      export CARGO_INCREMENTAL=0
    fi
    if [ "${{JERYU_SCCACHE_ENABLED:-1}}" != "0" ] && command -v sccache >/dev/null 2>&1; then
      export SCCACHE_DIR="$JERYU_CARGO_CACHE_ROOT/sccache"
      export RUSTC_WRAPPER=sccache
      export SCCACHE_NO_DAEMON=1
      if [ -n "${{JERYU_SCCACHE_CACHE_SIZE:-}}" ]; then
        export SCCACHE_CACHE_SIZE="$JERYU_SCCACHE_CACHE_SIZE"
      fi
      mkdir -p "$SCCACHE_DIR"
    else
      unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_NO_DAEMON SCCACHE_CACHE_SIZE
    fi
  fi
fi
"#
    )
}

pub fn write_lease(layout: &CargoCacheLayout) -> Result<Option<CargoLeaseGuard>> {
    let Some(lease_dir) = &layout.lease_dir else {
        return Ok(None);
    };

    let nonce = rand::random::<u64>();
    let path = lease_dir.join(format!("{}-{nonce:016x}.json", std::process::id()));
    let lease = CargoLeaseRecord {
        kind: "local-cargo".to_string(),
        scope_key: layout.scope_key.clone(),
        target_dir: layout.target_dir.display().to_string(),
        pid: std::process::id(),
        created_at: chrono::Utc::now().to_rfc3339(),
        rustc_key: layout.toolchain.rustc_key.clone(),
        rustc_version: layout.toolchain.rustc_version.clone(),
        host_triple: layout.toolchain.host_triple.clone(),
    };

    fs::create_dir_all(lease_dir).with_context(|| format!("creating {}", lease_dir.display()))?;
    fs::write(&path, serde_json::to_string_pretty(&lease)?)?;
    Ok(Some(CargoLeaseGuard {
        path,
        parent_dir: lease_dir.clone(),
    }))
}

pub fn lease_is_active(path: &Path) -> bool {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return false,
    };
    let Ok(record) = serde_json::from_str::<CargoLeaseRecord>(&raw) else {
        return false;
    };
    process_is_alive(record.pid)
}

pub fn scan_target_leases(target_dir: &Path) -> CargoLeaseScan {
    let lease_dir = target_dir.join(LEASES_DIR_NAME);
    let mut observed_files = 0;
    let mut stale_files = 0;
    let mut active = false;

    let Ok(entries) = fs::read_dir(&lease_dir) else {
        return CargoLeaseScan {
            active: false,
            observed_files: 0,
            stale_files: 0,
        };
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        observed_files += 1;
        if lease_is_active(&path) {
            active = true;
        } else {
            stale_files += 1;
            let _ = fs::remove_file(path);
        }
    }

    if !active && observed_files == stale_files {
        let _ = fs::remove_dir(&lease_dir);
    }

    CargoLeaseScan {
        active,
        observed_files,
        stale_files,
    }
}

pub fn shell_exports(layout: &CargoCacheLayout) -> Vec<String> {
    let mut lines = Vec::new();
    for (key, value) in &layout.env {
        lines.push(format!("export {}={}", key, shell_quote(value)));
    }
    if !layout.cargo_cache_enabled {
        lines.push("unset CARGO_TARGET_DIR".to_string());
    }
    if !layout.env.contains_key("RUSTC_WRAPPER") {
        lines.push(
            "unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_NO_DAEMON SCCACHE_CACHE_SIZE".to_string(),
        );
    }
    lines
}

fn process_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let pid = pid as i32;
        if pid <= 0 {
            return false;
        }
        // SAFETY: kill(0) checks for process existence without sending a signal.
        let rc = unsafe { libc::kill(pid, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[cfg(not(unix))]
    {
        pid > 0
    }
}

fn current_rustc_toolchain() -> Result<CargoToolchainKey> {
    let output = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .context("running rustc -vV")?;
    if !output.status.success() {
        anyhow::bail!("rustc -vV failed with status {:?}", output.status.code());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let rustc_version = stdout
        .lines()
        .next()
        .unwrap_or("rustc unknown")
        .trim()
        .to_string();
    let host_triple = stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap_or("unknown-host")
        .trim()
        .to_string();
    let rustc_key = short_hash(stdout.as_bytes());
    Ok(CargoToolchainKey {
        rustc_key,
        rustc_version,
        host_triple,
    })
}

fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | ' ' | '\t' | '\n' | '\r' => '_',
            other => other,
        })
        .collect()
}

fn short_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())[..12].to_string()
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn usable_sccache_binary() -> Option<PathBuf> {
    if !crate::settings::get().sccache.enabled {
        return None;
    }

    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("sccache");
        if is_usable_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_usable_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};
    use tempfile::TempDir;

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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

    fn make_test_bin_dir(
        include_cargo: bool,
        include_rustc: bool,
        include_sccache: bool,
    ) -> TempDir {
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
            fs::write(dir.path().join("sccache"), "#!/bin/sh\nexec \"$@\"\n").unwrap();
            let mut perms = fs::metadata(dir.path().join("sccache"))
                .unwrap()
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(dir.path().join("sccache"), perms).unwrap();
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
        let cache_root = PathBuf::from("/tmp/jeryu-cache");
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
            Path::new("/tmp/jeryu-cache"),
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
            cache_root: PathBuf::from("/tmp/root"),
            target_root: PathBuf::from("/tmp/root/targets"),
            target_dir: PathBuf::from("/tmp/root/targets/scope/target"),
            sccache_dir: PathBuf::from("/tmp/root/sccache"),
            toolchain: CargoToolchainKey {
                rustc_key: "abc".to_string(),
                rustc_version: "rustc 1.0.0".to_string(),
                host_triple: "x86_64-unknown-linux-gnu".to_string(),
            },
            cargo_cache_enabled: true,
            incremental_override: None,
            env: BTreeMap::from([("A".to_string(), "b c'd".to_string())]),
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
            Path::new("/tmp/jeryu-cache"),
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
        let lease_count = fs::read_dir(&lease_dir).unwrap().count();
        assert_eq!(lease_count, 2);

        drop(first);
        assert_eq!(fs::read_dir(&lease_dir).unwrap().count(), 1);
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
        fs::create_dir_all(&lease_dir).unwrap();
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
        fs::write(
            lease_dir.join("expired-a.json"),
            serde_json::to_vec_pretty(&expired).unwrap(),
        )
        .unwrap();
        fs::write(
            lease_dir.join("expired-b.json"),
            serde_json::to_vec_pretty(&expired).unwrap(),
        )
        .unwrap();
        fs::write(
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
            render_runner_cargo_pre_build_script(
                &pool_cache.path().display().to_string(),
                "docker",
            )
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
            render_runner_cargo_pre_build_script(
                &pool_cache.path().display().to_string(),
                "docker",
            )
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
}
