use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cargo_cache::{
    LEASES_DIR_NAME, current_rustc_toolchain, sanitize_segment, shell_quote, usable_sccache_binary,
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
    let pool_cache_mount = shell_quote(pool_cache_mount);
    let sccache_version = shell_quote(&crate::settings::get().sccache.binary_version);
    format!(
        r#"set -eu
JERYU_CARGO_PREREQS_OK=1
for tool in awk cat cut date mkdir rm rmdir sha256sum; do
  command -v "$tool" >/dev/null 2>&1 || JERYU_CARGO_PREREQS_OK=0
done
if [ "${{JERYU_CARGO_CACHE:-1}}" != "0" ] && [ "$JERYU_CARGO_PREREQS_OK" = "1" ] && command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
  RUSTC_INFO="$(rustc -vV)"
  HOST_TRIPLE="$(printf '%s\n' "$RUSTC_INFO" | awk '/^host: / {{ print $2; exit }}')"
  RUSTC_VERSION="$(printf '%s\n' "$RUSTC_INFO" | awk '/^release: / {{ print $2; exit }}')"
  if [ -n "$HOST_TRIPLE" ] && [ -n "$RUSTC_VERSION" ]; then
    RUSTC_KEY="$(printf '%s\n' "$RUSTC_INFO" | sha256sum | cut -c1-12)"
    JERYU_CARGO_SCOPE_KEY="${{CI_PROJECT_PATH_SLUG:-unknown-project}}"
    JERYU_CARGO_CACHE_ROOT={pool_cache_mount}
    JERYU_SCCACHE_VERSION={sccache_version}
    JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_CACHE_ROOT/cargo-targets/$JERYU_CARGO_SCOPE_KEY/$RUSTC_KEY/$HOST_TRIPLE"
    case "${{JERYU_CARGO_TARGET_ISOLATE:-slot}}" in
      shared|none)
        ;;
      id)
        JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_TARGET_ROOT/job-ids/${{CI_JOB_ID:-unknown}}"
        ;;
      slot|concurrent)
        JERYU_CARGO_MANAGER_KEY="${{CI_BUILDS_DIR:-}}"
        JERYU_CARGO_MANAGER_KEY="${{JERYU_CARGO_MANAGER_KEY##*/}}"
        if [ -z "$JERYU_CARGO_MANAGER_KEY" ]; then
          JERYU_CARGO_MANAGER_KEY="${{CI_RUNNER_SHORT_TOKEN:-${{CI_RUNNER_ID:-runner}}}}"
        fi
        JERYU_CARGO_SLOT_KEY="$JERYU_CARGO_MANAGER_KEY-${{CI_CONCURRENT_ID:-${{CI_CONCURRENT_PROJECT_ID:-0}}}}"
        JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_TARGET_ROOT/slots/$JERYU_CARGO_SLOT_KEY"
        ;;
      job|name|*)
        JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_TARGET_ROOT/jobs/${{CI_JOB_NAME_SLUG:-unknown}}"
        ;;
    esac
    export JERYU_CARGO_CACHE_ROOT JERYU_CARGO_SCOPE_KEY JERYU_CARGO_RUSTC_KEY="$RUSTC_KEY" JERYU_CARGO_RUSTC_VERSION="$RUSTC_VERSION" JERYU_CARGO_HOST_TRIPLE="$HOST_TRIPLE"
    export CARGO_TARGET_DIR="$JERYU_CARGO_TARGET_ROOT/target"
    mkdir -p "$CARGO_TARGET_DIR"
    JERYU_CARGO_LEASE_DIR="$CARGO_TARGET_DIR/{leases_dir}"
    mkdir -p "$JERYU_CARGO_LEASE_DIR"
    JERYU_CARGO_LEASE_FILE="$JERYU_CARGO_LEASE_DIR/${{CI_JOB_ID:-job}}-$$.json"
    cat > "$JERYU_CARGO_LEASE_FILE" <<EOF
{{"kind":"runner-cargo","scope_key":"$JERYU_CARGO_SCOPE_KEY","target_dir":"$CARGO_TARGET_DIR","pid":$$,"created_at":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","rustc_key":"$JERYU_CARGO_RUSTC_KEY","rustc_version":"$JERYU_CARGO_RUSTC_VERSION","host_triple":"$JERYU_CARGO_HOST_TRIPLE"}}
EOF
    trap 'rm -f "$JERYU_CARGO_LEASE_FILE"; rmdir "$JERYU_CARGO_LEASE_DIR" 2>/dev/null || true' EXIT
    if [ -n "${{JERYU_CARGO_INCREMENTAL:-}}" ]; then
      export CARGO_INCREMENTAL="$JERYU_CARGO_INCREMENTAL"
    else
      export CARGO_INCREMENTAL=0
    fi
    if [ "${{JERYU_CARGO_AUTOSIZE:-1}}" != "0" ]; then
      if [ -z "${{JERYU_CARGO_HOST_CORES:-}}" ]; then
        if command -v getconf >/dev/null 2>&1; then
          JERYU_CARGO_HOST_CORES="$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
        elif command -v nproc >/dev/null 2>&1; then
          JERYU_CARGO_HOST_CORES="$(nproc 2>/dev/null || true)"
        fi
      fi
      case "${{JERYU_CARGO_HOST_CORES:-}}" in
        ''|*[!0-9]*) JERYU_CARGO_HOST_CORES=4 ;;
      esac
      if [ -z "${{JERYU_CARGO_RESERVED_CORES:-}}" ]; then
        JERYU_CARGO_RESERVED_CORES=$((JERYU_CARGO_HOST_CORES / 4))
        if [ "$JERYU_CARGO_RESERVED_CORES" -lt 32 ]; then
          JERYU_CARGO_RESERVED_CORES=32
        fi
        JERYU_CARGO_HALF_CORES=$((JERYU_CARGO_HOST_CORES / 2))
        if [ "$JERYU_CARGO_RESERVED_CORES" -gt "$JERYU_CARGO_HALF_CORES" ]; then
          JERYU_CARGO_RESERVED_CORES="$JERYU_CARGO_HALF_CORES"
        fi
      fi
      case "$JERYU_CARGO_RESERVED_CORES" in
        ''|*[!0-9]*) JERYU_CARGO_RESERVED_CORES=1 ;;
      esac
      if [ "$JERYU_CARGO_RESERVED_CORES" -ge "$JERYU_CARGO_HOST_CORES" ]; then
        JERYU_CARGO_RESERVED_CORES=$((JERYU_CARGO_HOST_CORES - 1))
      fi
      if [ "$JERYU_CARGO_RESERVED_CORES" -lt 1 ]; then
        JERYU_CARGO_RESERVED_CORES=1
      fi
      JERYU_CARGO_TOTAL_SLOTS="${{JERYU_CARGO_TOTAL_RUNNER_SLOTS:-${{JERYU_RUNNER_FLEET_TOTAL_SLOTS:-${{JERYU_RUNNER_POOL_TOTAL_SLOTS:-${{JERYU_CARGO_RUNNER_SLOTS:-20}}}}}}}}"
      case "$JERYU_CARGO_TOTAL_SLOTS" in
        ''|*[!0-9]*) JERYU_CARGO_TOTAL_SLOTS=20 ;;
      esac
      if [ "$JERYU_CARGO_TOTAL_SLOTS" -lt 1 ]; then
        JERYU_CARGO_TOTAL_SLOTS=1
      fi
      JERYU_CARGO_USABLE_CORES=$((JERYU_CARGO_HOST_CORES - JERYU_CARGO_RESERVED_CORES))
      if [ "$JERYU_CARGO_USABLE_CORES" -lt 1 ]; then
        JERYU_CARGO_USABLE_CORES=1
      fi
      JERYU_CARGO_AUTO_BUILD_JOBS=$((JERYU_CARGO_USABLE_CORES / JERYU_CARGO_TOTAL_SLOTS))
      if [ "$JERYU_CARGO_AUTO_BUILD_JOBS" -lt "${{JERYU_CARGO_MIN_BUILD_JOBS:-1}}" ]; then
        JERYU_CARGO_AUTO_BUILD_JOBS="${{JERYU_CARGO_MIN_BUILD_JOBS:-1}}"
      fi
      if [ "$JERYU_CARGO_AUTO_BUILD_JOBS" -gt "${{JERYU_CARGO_MAX_BUILD_JOBS:-16}}" ]; then
        JERYU_CARGO_AUTO_BUILD_JOBS="${{JERYU_CARGO_MAX_BUILD_JOBS:-16}}"
      fi
      case "${{CARGO_BUILD_JOBS:-}}" in
        ''|*[!0-9]*)
          export CARGO_BUILD_JOBS="$JERYU_CARGO_AUTO_BUILD_JOBS"
          ;;
        *)
          if [ "$CARGO_BUILD_JOBS" -gt "$JERYU_CARGO_AUTO_BUILD_JOBS" ]; then
            export CARGO_BUILD_JOBS="$JERYU_CARGO_AUTO_BUILD_JOBS"
          fi
          ;;
      esac
      export JERYU_CARGO_HOST_CORES JERYU_CARGO_RESERVED_CORES JERYU_CARGO_TOTAL_SLOTS JERYU_CARGO_AUTO_BUILD_JOBS
    fi
    if [ "${{JERYU_SCCACHE_ENABLED:-1}}" != "0" ] && ! command -v sccache >/dev/null 2>&1; then
      JERYU_TOOLS_DIR="$JERYU_CARGO_CACHE_ROOT/tools"
      JERYU_SCCACHE_BIN="$JERYU_TOOLS_DIR/sccache"
      if [ ! -x "$JERYU_SCCACHE_BIN" ] && command -v curl >/dev/null 2>&1 && command -v tar >/dev/null 2>&1; then
        mkdir -p "$JERYU_TOOLS_DIR/.tmp"
        JERYU_SCCACHE_TMP="$JERYU_TOOLS_DIR/.tmp/sccache-$JERYU_SCCACHE_VERSION-$$.tar.gz"
        JERYU_SCCACHE_EXTRACT="$JERYU_TOOLS_DIR/.tmp/sccache-$JERYU_SCCACHE_VERSION-$$"
        rm -rf "$JERYU_SCCACHE_EXTRACT"
        mkdir -p "$JERYU_SCCACHE_EXTRACT"
        if curl -fsSL "https://github.com/mozilla/sccache/releases/download/$JERYU_SCCACHE_VERSION/sccache-$JERYU_SCCACHE_VERSION-x86_64-unknown-linux-musl.tar.gz" -o "$JERYU_SCCACHE_TMP"; then
          if tar -xzf "$JERYU_SCCACHE_TMP" -C "$JERYU_SCCACHE_EXTRACT" "sccache-$JERYU_SCCACHE_VERSION-x86_64-unknown-linux-musl/sccache" 2>/dev/null; then
            if mv "$JERYU_SCCACHE_EXTRACT/sccache-$JERYU_SCCACHE_VERSION-x86_64-unknown-linux-musl/sccache" "$JERYU_SCCACHE_BIN" && chmod 0755 "$JERYU_SCCACHE_BIN"; then
              :
            else
              rm -f "$JERYU_SCCACHE_BIN"
            fi
          fi
        fi
        rm -f "$JERYU_SCCACHE_TMP"
        rm -rf "$JERYU_SCCACHE_EXTRACT"
      fi
      if [ -x "$JERYU_SCCACHE_BIN" ]; then
        export PATH="$JERYU_TOOLS_DIR:$PATH"
      fi
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
"#,
        leases_dir = LEASES_DIR_NAME,
        pool_cache_mount = pool_cache_mount,
        sccache_version = sccache_version,
    )
}
