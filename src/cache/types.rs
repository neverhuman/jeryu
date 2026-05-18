use super::*;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Typed errors for SmartCache lifecycle.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("warp-registry failed to start: {0}")]
    RegistryFailed(String),
    #[error("Invalid Docker configuration written, rollback applied")]
    DockerConfigInvalid,
    #[error("Docker restart failed, rollback applied")]
    DockerRestartFailed,
    #[error("Docker daemon configuration validation failed")]
    DockerValidationFailed,
    #[error("df failed: {0}")]
    DfFailed(String),
    #[error("unexpected df output: {0}")]
    UnexpectedDfOutput(String),
    #[error("empty age")]
    EmptyAge,
    #[error("unsupported age '{0}'; use suffix m, h, or d")]
    UnsupportedAge(String),
    #[error("SmartCache health checks failed: proxy={0}, reg={1}, disk={2}")]
    HealthCheckFailed(bool, bool, bool),
    #[error("Corrupted data during CAS ingestion: mismatched hashes. given={0} computed={1}")]
    CasHashMismatch(String, String),
    #[error("docker system df failed: {0}")]
    DockerDfFailed(String),
    #[error("manager cache cleanup failed: {0}")]
    CleanupFailed(String),
}

pub const ROOT_DISK_HEADROOM_MIN_FREE_BYTES: u64 = 80 * 1024 * 1024 * 1024;
pub const ROOT_DISK_WARNING_MIN_FREE_BYTES: u64 = ROOT_DISK_HEADROOM_MIN_FREE_BYTES;
pub const ROOT_DISK_CRITICAL_MIN_FREE_BYTES: u64 = 60 * 1024 * 1024 * 1024;
pub const ROOT_DISK_EMERGENCY_MIN_FREE_BYTES: u64 = 40 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskPressureLevel {
    Nominal,
    Warning,
    Critical,
    Emergency,
}

pub fn root_disk_pressure_level(available_bytes: u64) -> DiskPressureLevel {
    if available_bytes < ROOT_DISK_EMERGENCY_MIN_FREE_BYTES {
        DiskPressureLevel::Emergency
    } else if available_bytes < ROOT_DISK_CRITICAL_MIN_FREE_BYTES {
        DiskPressureLevel::Critical
    } else if available_bytes < ROOT_DISK_WARNING_MIN_FREE_BYTES {
        DiskPressureLevel::Warning
    } else {
        DiskPressureLevel::Nominal
    }
}

pub struct SmartCache {
    pub(crate) db: crate::state::Db,
    pub(crate) proxy_port: u16,
    pub(crate) registry_port: u16,
}

#[derive(Clone, Default)]
pub struct CacheManager;

#[derive(Clone, Debug)]
pub struct GcOptions {
    pub dry_run: bool,
    pub json: bool,
    pub keep_active_managers: bool,
    pub older_than: Option<String>,
    pub max_cache_gb: Option<f64>,
    pub quiet: bool,
}

impl Default for GcOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            json: false,
            keep_active_managers: true,
            older_than: None,
            max_cache_gb: None,
            quiet: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheStatusReport {
    pub generated_at: String,
    pub root_fs: FsUsage,
    pub jeryu_cache_bytes: u64,
    pub manager_cache_bytes: u64,
    pub manager_cache_budget_bytes: Option<u64>,
    pub manager_caches: Vec<ManagerCacheStatus>,
    pub manager_cargo_target_bytes: u64,
    pub manager_cargo_sccache_bytes: u64,
    pub local_cargo_target_bytes: u64,
    pub local_cargo_sccache_bytes: u64,
    pub pool_cargo_target_bytes: u64,
    pub pool_cargo_sccache_bytes: u64,
    pub cargo_target_caches: Vec<CargoTargetCacheStatus>,
    pub docker: DockerStorageSummary,
    pub proxy_up: bool,
    pub registry_up: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FsUsage {
    pub path: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManagerCacheStatus {
    pub manager_id: String,
    pub path: String,
    pub bytes: u64,
    pub sccache_bytes: u64,
    pub active: bool,
    pub age_seconds: Option<u64>,
    pub gc_candidate: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CargoTargetCacheStatus {
    pub scope: String,
    pub path: String,
    pub bytes: u64,
    pub active: bool,
    pub lease_observed: bool,
    pub age_seconds: Option<u64>,
    pub gc_candidate: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DockerStorageSummary {
    pub images: Option<DockerStorageClass>,
    pub containers: Option<DockerStorageClass>,
    pub local_volumes: Option<DockerStorageClass>,
    pub build_cache: Option<DockerStorageClass>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DockerStorageClass {
    pub total_count: Option<u64>,
    pub active_count: Option<u64>,
    pub size: String,
    pub reclaimable: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheGcReport {
    pub dry_run: bool,
    pub removed_manager_caches: Vec<String>,
    pub candidate_manager_caches: Vec<ManagerCacheStatus>,
    pub removed_cargo_targets: Vec<String>,
    pub candidate_cargo_targets: Vec<CargoTargetCacheStatus>,
    pub gc_eviction_count: u64,
    pub errors: Vec<String>,
}

pub async fn ensure_root_disk_headroom(required_free_bytes: u64, operation: &str) -> Result<()> {
    let usage = crate::cache::df_usage("/").await?;
    if usage.available_bytes < required_free_bytes {
        anyhow::bail!(
            "{operation} blocked: {} has {} free, need at least {}",
            usage.path,
            crate::cache::human_bytes(usage.available_bytes),
            crate::cache::human_bytes(required_free_bytes)
        );
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostDoctorReport {
    pub generated_at: String,
    pub ok: bool,
    pub checks: Vec<HostDoctorCheck>,
    pub cache: CacheStatusReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostDoctorCheck {
    pub id: String,
    pub ok: bool,
    pub detail: String,
}
