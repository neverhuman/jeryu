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
    let cargo_home_dir = cache_root.join(crate::cargo_cache::CACHE_HOME_DIR_NAME);
    let rustup_home_dir = cache_root.join(crate::cargo_cache::RUSTUP_HOME_DIR_NAME);
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
    env.insert(
        "JERYU_CARGO_TARGET_PROFILE".to_string(),
        std::env::var("JERYU_CARGO_TARGET_PROFILE").unwrap_or_else(|_| "debug".to_string()),
    );
    env.insert("CARGO_INCREMENTAL".to_string(), cargo_incremental.clone());

    if cache_enabled {
        env.insert(
            "CARGO_TARGET_DIR".to_string(),
            target_dir.display().to_string(),
        );
        env.insert(
            "CARGO_HOME".to_string(),
            cargo_home_dir.display().to_string(),
        );
        env.insert(
            "RUSTUP_HOME".to_string(),
            rustup_home_dir.display().to_string(),
        );
        env.insert("SCCACHE_DIR".to_string(), sccache_dir.display().to_string());
        env.insert("SCCACHE_NO_DAEMON".to_string(), "1".to_string());
        env.insert(
            "SCCACHE_CACHE_SIZE".to_string(),
            crate::settings::get().sccache.cache_size.clone(),
        );
    }

    if let Some(sccache_binary) = usable_sccache_binary() {
        env.insert(
            "RUSTC_WRAPPER".to_string(),
            sccache_binary.display().to_string(),
        );
    } else if let Some(existing_wrapper) = std::env::var_os("RUSTC_WRAPPER") {
        env.insert(
            "RUSTC_WRAPPER".to_string(),
            existing_wrapper.to_string_lossy().to_string(),
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
