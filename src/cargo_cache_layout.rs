use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cargo_cache::{
    LEASES_DIR_NAME, current_rustc_toolchain, sanitize_segment, usable_sccache_binary,
};

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
    Ok(super::short_hash(canonical.to_string_lossy().as_bytes()))
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
