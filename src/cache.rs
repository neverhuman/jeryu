//! Owner: SmartCache & Disk Management
//! Proof: `cargo test -p jeryu -- cache`
//! Invariants: LRU GC every 30 min; active-manager caches never collected; CAS atomic store

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tracing::info;
use walkdir::WalkDir;

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

const MIN_GITLAB_ARTIFACT_SIZE_MB: u64 = 4096;
const POOL_TARGET_LEASE_RECOVERY_TTL_SECS: u64 = 2 * 60 * 60;
const NEXTEST_EXTRACT_FALLBACK_TTL_SECS: u64 = 2 * 60 * 60;
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
    db: crate::state::Db,
    proxy_port: u16,
    registry_port: u16,
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
    pub deleted_manager_caches: Vec<String>,
    pub candidate_manager_caches: Vec<ManagerCacheStatus>,
    pub deleted_cargo_targets: Vec<String>,
    pub candidate_cargo_targets: Vec<CargoTargetCacheStatus>,
    pub reclaimed_cache_request_rows: u64,
    pub errors: Vec<String>,
}

pub async fn ensure_root_disk_headroom(required_free_bytes: u64, operation: &str) -> Result<()> {
    let usage = df_usage("/").await?;
    if usage.available_bytes < required_free_bytes {
        anyhow::bail!(
            "{operation} blocked: {} has {} free, need at least {}",
            usage.path,
            human_bytes(usage.available_bytes),
            human_bytes(required_free_bytes)
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

impl SmartCache {
    pub fn new(db: crate::state::Db) -> Self {
        Self {
            db,
            proxy_port: crate::config::CACHE_PROXY_PORT,
            registry_port: crate::config::CACHE_REGISTRY_PORT,
        }
    }

    pub async fn start(self) -> Result<()> {
        info!("Starting SmartCache supervisor...");
        self.start_warp_registry().await?;

        let proxy = std::sync::Arc::new(crate::cache_proxy::CacheProxy::new(
            self.proxy_port,
            self.db.clone(),
        ));
        tokio::spawn(async move {
            if let Err(e) = proxy.start().await {
                tracing::error!("warp-proxy failed: {:?}", e);
            }
        });

        Ok(())
    }

    async fn start_warp_registry(&self) -> Result<()> {
        info!(
            "Ensuring warp-registry container is running on 127.0.0.1:{}",
            self.registry_port
        );

        // Stop and remove existing to be clean, or just check if it exists.
        // For simplicity, we just run docker run with --rm or --restart unless-stopped.
        let output = tokio::process::Command::new("docker")
            .args(["ps", "-q", "-f", "name=warp-registry"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            info!("warp-registry is already running");
            return Ok(());
        }

        let output = tokio::process::Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                "warp-registry",
                &format!("-p=0.0.0.0:{}:5000", self.registry_port),
                "--restart",
                "always",
                "-e",
                "REGISTRY_PROXY_REMOTEURL=https://registry-1.docker.io",
                "registry:2",
            ])
            .output()
            .await
            .context("Failed to start warp-registry")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(CacheError::RegistryFailed(err.into_owned()).into());
        }

        info!("Started warp-registry container");
        Ok(())
    }

    pub async fn enable(&self) -> Result<()> {
        println!("🔧 Enabling SmartCache Docker mirror...");
        let daemon_json = std::path::Path::new("/etc/docker/daemon.json");
        let mut config = if daemon_json.exists() {
            let content = std::fs::read_to_string(daemon_json)?;
            std::fs::write("/etc/docker/daemon.json.bak", &content)?;
            serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if let Some(obj) = config.as_object_mut() {
            let mirror = serde_json::json!([format!("http://127.0.0.1:{}", self.registry_port)]);
            obj.insert("registry-mirrors".to_string(), mirror);
        }

        std::fs::write(daemon_json, serde_json::to_string_pretty(&config)?)?;

        let valid = tokio::process::Command::new("sudo")
            .args([
                "dockerd",
                "--validate",
                "--config-file",
                daemon_json.to_str().unwrap(),
            ])
            .status()
            .await?;

        if !valid.success() {
            println!("Docker config validation failed, rolling back...");
            if std::path::Path::new("/etc/docker/daemon.json.bak").exists() {
                std::fs::copy("/etc/docker/daemon.json.bak", "/etc/docker/daemon.json")?;
            }
            return Err(CacheError::DockerConfigInvalid.into());
        }

        println!("Restarting Docker daemon...");
        let status = tokio::process::Command::new("sudo")
            .args(["systemctl", "restart", "docker"])
            .status()
            .await?;

        if !status.success() {
            println!("Docker failed to start, rolling back...");
            if std::path::Path::new("/etc/docker/daemon.json.bak").exists() {
                std::fs::copy("/etc/docker/daemon.json.bak", "/etc/docker/daemon.json")?;
                let _ = tokio::process::Command::new("sudo")
                    .args(["systemctl", "restart", "docker"])
                    .status()
                    .await;
            }
            return Err(CacheError::DockerRestartFailed.into());
        }

        println!("✅ SmartCache Docker mirror enabled");
        Ok(())
    }

    pub async fn doctor(&self) -> Result<()> {
        println!("🩺 Running SmartCache doctor...");
        println!("Checking proxy reachability ({})...", self.proxy_port);
        let proxy_up = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", self.proxy_port))
            .await
            .is_ok();
        println!("Checking registry mirror ({})...", self.registry_port);
        let reg_up = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", self.registry_port))
            .await
            .is_ok();

        println!("Checking local cache directory writeability...");
        let cache_dir = crate::config::data_dir().join("cache");
        std::fs::create_dir_all(&cache_dir)?;
        let test_file = cache_dir.join(".doctor_test");
        let disk_ok = std::fs::write(&test_file, b"ok").is_ok();
        let _ = std::fs::remove_file(test_file);

        if proxy_up && reg_up && disk_ok {
            println!("✅ SmartCache is healthy");
        } else {
            return Err(CacheError::HealthCheckFailed(proxy_up, reg_up, disk_ok).into());
        }
        Ok(())
    }

    pub async fn status(&self) -> Result<()> {
        self.status_with_options(false).await
    }

    pub async fn status_with_options(&self, json: bool) -> Result<()> {
        let report = self.status_report(None).await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            print_cache_status_report(&report);
        }
        Ok(())
    }

    pub async fn gc(&self) -> Result<()> {
        self.gc_with_options(GcOptions::default()).await.map(|_| ())
    }

    pub async fn gc_with_options(&self, options: GcOptions) -> Result<CacheGcReport> {
        let budget_bytes = options.max_cache_gb.map(gb_to_bytes);
        let mut status = self.status_report(budget_bytes).await?;
        let max_age = match options.older_than.as_deref() {
            Some(raw) => Some(parse_age(raw)?),
            None => None,
        };
        let total_cache_bytes = status.manager_cache_bytes
            + status.local_cargo_target_bytes
            + status.pool_cargo_target_bytes
            + status.local_cargo_sccache_bytes
            + status.pool_cargo_sccache_bytes;
        let over_budget = budget_bytes
            .map(|budget| total_cache_bytes > budget)
            .unwrap_or(false);

        for cache in &mut status.manager_caches {
            // Only skip active managers when explicitly asked to preserve them.
            // When keep_active_managers=false (emergency/critical pressure), fall through
            // to normal age/budget logic so active caches can be evicted.
            if cache.active && options.keep_active_managers {
                cache.gc_candidate = false;
                cache.reason = "active manager cache preserved".to_string();
                continue;
            }
            let old_enough = max_age
                .and_then(|age| cache.age_seconds.map(|seconds| seconds >= age.as_secs()))
                .unwrap_or(false);
            if max_age.is_none() || old_enough || over_budget {
                cache.gc_candidate = true;
                cache.reason = if cache.active {
                    if over_budget {
                        "active manager cache evicted: over global budget".to_string()
                    } else {
                        "active manager cache evicted: older than threshold".to_string()
                    }
                } else if over_budget {
                    "orphan manager cache selected because cache is over budget".to_string()
                } else if max_age.is_some() {
                    "orphan manager cache older than threshold".to_string()
                } else {
                    "orphan manager cache".to_string()
                };
            }
        }

        let candidates: Vec<ManagerCacheStatus> = status
            .manager_caches
            .iter()
            .filter(|cache| cache.gc_candidate)
            .cloned()
            .collect();
        for cache in &mut status.cargo_target_caches {
            if cache.active {
                cache.gc_candidate = false;
                cache.reason = "active cargo target cache preserved".to_string();
                continue;
            }
            let old_enough = max_age
                .and_then(|age| cache.age_seconds.map(|seconds| seconds >= age.as_secs()))
                .unwrap_or(false);
            if max_age.is_none() || old_enough || over_budget {
                cache.gc_candidate = true;
                cache.reason = if over_budget {
                    "cargo target cache selected because cache is over budget".to_string()
                } else if max_age.is_some() {
                    "cargo target cache older than threshold".to_string()
                } else {
                    "cargo target cache".to_string()
                };
            }
        }
        let cargo_candidates: Vec<CargoTargetCacheStatus> = status
            .cargo_target_caches
            .iter()
            .filter(|cache| cache.gc_candidate)
            .cloned()
            .collect();
        let mut deleted = Vec::new();
        let mut errors = Vec::new();
        let mut deleted_cargo = Vec::new();

        if !options.dry_run && !candidates.is_empty() {
            match remove_manager_cache_dirs_as_root(&candidates).await {
                Ok(removed) => deleted = removed,
                Err(err) => errors.push(err.to_string()),
            }
        }
        if !options.dry_run && !cargo_candidates.is_empty() {
            let paths: Vec<PathBuf> = cargo_candidates
                .iter()
                .map(|cache| PathBuf::from(&cache.path))
                .collect();
            match remove_cache_paths_as_root(&crate::config::cache_root_dir(), &paths).await {
                Ok(removed) => deleted_cargo = removed,
                Err(err) => errors.push(err.to_string()),
            }
        }

        let reclaimed = self.db.prune_cache_requests(7).await?;
        if !options.dry_run {
            let cutoff = (Utc::now() - ChronoDuration::days(7)).to_rfc3339();
            let _ = self.db.prune_test_verdicts(&cutoff).await?;
            let _ = self.db.prune_action_cache(&cutoff).await?;
        }
        let report = CacheGcReport {
            dry_run: options.dry_run,
            deleted_manager_caches: deleted,
            candidate_manager_caches: candidates,
            deleted_cargo_targets: deleted_cargo,
            candidate_cargo_targets: cargo_candidates,
            reclaimed_cache_request_rows: reclaimed,
            errors,
        };

        if !options.quiet {
            if options.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_cache_gc_report(&report);
            }
        }
        Ok(report)
    }

    pub async fn host_doctor_report(&self) -> Result<HostDoctorReport> {
        let cache = self.status_report(Some(gb_to_bytes(400.0))).await?;
        let mut checks = Vec::new();

        checks.push(HostDoctorCheck {
            id: "root-disk-free".to_string(),
            ok: cache.root_fs.available_bytes >= ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
            detail: format!(
                "{} free on {}",
                human_bytes(cache.root_fs.available_bytes),
                cache.root_fs.path
            ),
        });
        checks.push(HostDoctorCheck {
            id: "runner-cache-budget".to_string(),
            ok: cache.manager_cache_bytes <= gb_to_bytes(400.0),
            detail: format!(
                "{} in manager caches",
                human_bytes(cache.manager_cache_bytes)
            ),
        });
        checks.push(HostDoctorCheck {
            id: "smartcache-proxy".to_string(),
            ok: cache.proxy_up,
            detail: format!(
                "proxy {}",
                if cache.proxy_up { "reachable" } else { "down" }
            ),
        });
        checks.push(HostDoctorCheck {
            id: "smartcache-registry".to_string(),
            ok: cache.registry_up,
            detail: format!(
                "registry mirror {}",
                if cache.registry_up {
                    "reachable"
                } else {
                    "down"
                }
            ),
        });
        checks.push(gitlab_redis_write_check().await);
        checks.push(gitlab_artifact_size_check().await);

        let ok = checks.iter().all(|check| check.ok);
        Ok(HostDoctorReport {
            generated_at: now_rfc3339(),
            ok,
            checks,
            cache,
        })
    }

    pub async fn status_report(&self, budget_bytes: Option<u64>) -> Result<CacheStatusReport> {
        let proxy_up = tcp_up(self.proxy_port).await;
        let registry_up = tcp_up(self.registry_port).await;
        let active_managers = active_runner_manager_ids().await;
        let mut active_pool_names: BTreeSet<String> = BTreeSet::new();
        for pool in self.db.list_pools().await? {
            if self.db.count_active_managers(&pool.name).await.unwrap_or(0) > 0 {
                active_pool_names.insert(pool.name);
            }
        }
        let manager_root = crate::config::cache_root_dir().join("managers");
        let mut manager_caches = Vec::new();

        if manager_root.is_dir() {
            for entry in std::fs::read_dir(&manager_root)
                .with_context(|| format!("reading {}", manager_root.display()))?
            {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let manager_id = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                let active = active_managers.contains(&manager_id);
                let bytes = du_bytes(&path).await.unwrap_or(0);
                let sccache_bytes = du_bytes(&path.join("sccache")).await.unwrap_or(0);
                let age_seconds = path_age_seconds(&path);
                manager_caches.push(ManagerCacheStatus {
                    manager_id,
                    path: path.display().to_string(),
                    bytes,
                    sccache_bytes,
                    active,
                    age_seconds,
                    gc_candidate: false,
                    reason: if active {
                        "active manager cache".to_string()
                    } else {
                        "orphan manager cache".to_string()
                    },
                });
            }
        }

        manager_caches.sort_by(|a, b| {
            b.bytes
                .cmp(&a.bytes)
                .then_with(|| a.manager_id.cmp(&b.manager_id))
        });

        let local_cargo_targets =
            scan_cargo_target_dirs(&crate::config::local_cargo_targets_root(), "local").await?;
        let local_cargo_target_bytes = local_cargo_targets.iter().map(|cache| cache.bytes).sum();
        let local_cargo_sccache_bytes = du_bytes(&crate::config::local_cargo_sccache_dir())
            .await
            .unwrap_or(0);

        let mut cargo_target_caches = local_cargo_targets;
        let pool_root = crate::config::cache_root_dir().join("pools");
        let mut pool_cargo_target_bytes = 0_u64;
        let mut pool_cargo_sccache_bytes = 0_u64;
        if pool_root.is_dir() {
            for entry in std::fs::read_dir(&pool_root)
                .with_context(|| format!("reading {}", pool_root.display()))?
            {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let pool_name = entry.file_name().to_string_lossy().to_string();
                let pool_cache_dir = entry.path();
                let mut statuses = scan_cargo_target_dirs(
                    &pool_cache_dir.join("cargo-targets"),
                    &format!("pool:{pool_name}"),
                )
                .await?;
                if active_pool_names.contains(&pool_name) {
                    for status in &mut statuses {
                        if status.active {
                            continue;
                        }
                        let ttl = pool_target_lease_recovery_ttl().as_secs();
                        let recovery_active = !status.lease_observed
                            && status.age_seconds.map(|age| age <= ttl).unwrap_or(true);
                        if recovery_active {
                            status.active = true;
                            status.reason =
                                "active pool cargo cache recovery path (lease absent)".to_string();
                        } else if !status.lease_observed {
                            status.reason = "pool cargo cache without lease".to_string();
                        }
                    }
                }
                pool_cargo_target_bytes += statuses.iter().map(|status| status.bytes).sum::<u64>();
                pool_cargo_sccache_bytes +=
                    du_bytes(&pool_cache_dir.join("sccache")).await.unwrap_or(0);
                cargo_target_caches.extend(statuses);
            }
        }
        cargo_target_caches.sort_by(|a, b| {
            b.bytes
                .cmp(&a.bytes)
                .then_with(|| a.scope.cmp(&b.scope))
                .then_with(|| a.path.cmp(&b.path))
        });

        let manager_cache_bytes = manager_caches.iter().map(|cache| cache.bytes).sum();
        Ok(CacheStatusReport {
            generated_at: now_rfc3339(),
            root_fs: df_usage("/").await?,
            jeryu_cache_bytes: du_bytes(&crate::config::cache_root_dir())
                .await
                .unwrap_or(0),
            manager_cache_bytes,
            manager_cache_budget_bytes: budget_bytes,
            manager_caches,
            local_cargo_target_bytes,
            local_cargo_sccache_bytes,
            pool_cargo_target_bytes,
            pool_cargo_sccache_bytes,
            cargo_target_caches,
            docker: match docker_storage_summary().await {
                Ok(summary) => summary,
                Err(_) => DockerStorageSummary::default(),
            },
            proxy_up,
            registry_up,
        })
    }

    /// Store a blob in CAS with Scratch -> Hash -> Fsync -> Rename safety pattern
    pub async fn atomic_store_cas(data: &[u8], digest: &str) -> Result<()> {
        let cas_dir = crate::config::data_dir().join("cas");
        tokio::fs::create_dir_all(&cas_dir).await?;

        let path = cas_dir.join(digest);
        if path.exists() {
            return Ok(());
        }

        let scratch_path = cas_dir.join(format!("{}.tmp", digest));

        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&scratch_path)
            .await?;

        // Verify hash actually matches the content before saving
        use sha2::Digest;
        let result = sha2::Sha256::digest(data);
        let computed_digest = hex::encode(result);
        if computed_digest != digest {
            return Err(CacheError::CasHashMismatch(digest.to_string(), computed_digest).into());
        }

        file.write_all(data).await?;
        // fsync to persist before directory entry
        file.sync_all().await?;

        // Atomic rename
        tokio::fs::rename(scratch_path, path).await?;
        Ok(())
    }
}

pub fn print_cache_status_report(report: &CacheStatusReport) {
    println!("📊 SmartCache Status");
    println!(
        "Root FS: {} free / {} total ({:.1}% used)",
        human_bytes(report.root_fs.available_bytes),
        human_bytes(report.root_fs.total_bytes),
        report.root_fs.used_percent
    );
    println!("Proxy: {}", if report.proxy_up { "Up" } else { "Down" });
    println!(
        "Registry Mirror: {}",
        if report.registry_up { "Up" } else { "Down" }
    );
    println!(
        "jeryu cache: {} (manager caches: {})",
        human_bytes(report.jeryu_cache_bytes),
        human_bytes(report.manager_cache_bytes)
    );
    println!(
        "Cargo targets: local={} pool={}",
        human_bytes(report.local_cargo_target_bytes),
        human_bytes(report.pool_cargo_target_bytes)
    );
    println!(
        "Sccache dirs:  local={} pool={}",
        human_bytes(report.local_cargo_sccache_bytes),
        human_bytes(report.pool_cargo_sccache_bytes)
    );
    let orphan_count = report
        .manager_caches
        .iter()
        .filter(|cache| !cache.active)
        .count();
    println!(
        "Manager caches: {} total, {} orphaned",
        report.manager_caches.len(),
        orphan_count
    );
    for cache in report.manager_caches.iter().take(12) {
        let sccache_info = if cache.sccache_bytes > 0 {
            format!(" (sccache: {})", human_bytes(cache.sccache_bytes))
        } else {
            String::new()
        };
        println!(
            "  {:<36} {:>9}{} {}",
            cache.manager_id,
            human_bytes(cache.bytes),
            sccache_info,
            if cache.active { "active" } else { "orphan" }
        );
    }
    let cargo_orphans = report
        .cargo_target_caches
        .iter()
        .filter(|cache| !cache.active)
        .count();
    println!(
        "Cargo targets: {} total, {} orphaned",
        report.cargo_target_caches.len(),
        cargo_orphans
    );
    for cache in report.cargo_target_caches.iter().take(12) {
        println!(
            "  {:<36} {:>9} {}",
            cache.scope,
            human_bytes(cache.bytes),
            if cache.active {
                "active"
            } else {
                cache.reason.as_str()
            }
        );
    }
}

pub fn print_cache_gc_report(report: &CacheGcReport) {
    println!("🧹 SmartCache GC");
    println!("Dry run: {}", report.dry_run);
    println!("Candidates: {}", report.candidate_manager_caches.len());
    for cache in &report.candidate_manager_caches {
        println!(
            "  {:<36} {:>9} {}",
            cache.manager_id,
            human_bytes(cache.bytes),
            cache.reason
        );
    }
    if !report.deleted_manager_caches.is_empty() {
        println!("Deleted: {}", report.deleted_manager_caches.len());
    }
    if !report.candidate_cargo_targets.is_empty() {
        println!("Cargo candidates: {}", report.candidate_cargo_targets.len());
        for cache in &report.candidate_cargo_targets {
            println!(
                "  {:<36} {:>9} {}",
                cache.scope,
                human_bytes(cache.bytes),
                cache.reason
            );
        }
    }
    if !report.deleted_cargo_targets.is_empty() {
        println!("Cargo deleted: {}", report.deleted_cargo_targets.len());
    }
    if report.reclaimed_cache_request_rows > 0 {
        println!(
            "Pruned cache request rows: {}",
            report.reclaimed_cache_request_rows
        );
    }
    for err in &report.errors {
        println!("Warning: {err}");
    }
}

pub fn print_host_doctor_report(report: &HostDoctorReport) {
    println!("━━━ jeryu host doctor ━━━");
    for check in &report.checks {
        println!(
            "{} {:<24} {}",
            if check.ok { "✅" } else { "❌" },
            check.id,
            check.detail
        );
    }
}

async fn tcp_up(port: u16) -> bool {
    tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .is_ok()
}

async fn active_runner_manager_ids() -> BTreeSet<String> {
    let output = tokio::process::Command::new("docker")
        .args(["ps", "--format", "{{.Names}}"])
        .output()
        .await;
    let Ok(output) = output else {
        return BTreeSet::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("jeryu-runner-"))
        .map(str::to_string)
        .collect()
}

async fn du_bytes(path: &Path) -> Result<u64> {
    let output = tokio::process::Command::new("du")
        .args(["-sb", &path.display().to_string()])
        .output()
        .await
        .with_context(|| format!("du -sb {}", path.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(first) = stdout.split_whitespace().next() else {
        return Ok(0);
    };
    Ok(first.parse::<u64>().unwrap_or(0))
}

pub(crate) async fn df_usage(path: &str) -> Result<FsUsage> {
    let output = tokio::process::Command::new("df")
        .args(["-Pk", path])
        .output()
        .await
        .context("df -Pk")?;
    if !output.status.success() {
        return Err(
            CacheError::DfFailed(String::from_utf8_lossy(&output.stderr).into_owned()).into(),
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("df output missing data row"))?;
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 6 {
        return Err(CacheError::UnexpectedDfOutput(line.to_string()).into());
    }
    let total_bytes = fields[1].parse::<u64>().unwrap_or(0) * 1024;
    let used_bytes = fields[2].parse::<u64>().unwrap_or(0) * 1024;
    let available_bytes = fields[3].parse::<u64>().unwrap_or(0) * 1024;
    let used_percent = fields[4]
        .trim_end_matches('%')
        .parse::<f64>()
        .unwrap_or(0.0);
    Ok(FsUsage {
        path: fields[5].to_string(),
        total_bytes,
        used_bytes,
        available_bytes,
        used_percent,
    })
}

async fn docker_storage_summary() -> Result<DockerStorageSummary> {
    let output = tokio::process::Command::new("docker")
        .args(["system", "df", "--format", "{{json .}}"])
        .output()
        .await
        .context("docker system df")?;
    if !output.status.success() {
        return Err(CacheError::DockerDfFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
        .into());
    }

    let mut summary = DockerStorageSummary::default();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let value: serde_json::Value = serde_json::from_str(line)?;
        let class = DockerStorageClass {
            total_count: json_u64(&value, "TotalCount"),
            active_count: json_u64(&value, "Active"),
            size: json_string(&value, "Size"),
            reclaimable: json_string(&value, "Reclaimable"),
        };
        match json_string(&value, "Type").as_str() {
            "Images" => summary.images = Some(class),
            "Containers" => summary.containers = Some(class),
            "Local Volumes" => summary.local_volumes = Some(class),
            "Build Cache" => summary.build_cache = Some(class),
            _ => {}
        }
    }
    Ok(summary)
}

fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
    })
}

fn path_age_seconds(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|duration| duration.as_secs())
}

fn pool_target_lease_recovery_ttl() -> Duration {
    std::env::var("JERYU_POOL_CARGO_LEASE_RECOVERY_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(POOL_TARGET_LEASE_RECOVERY_TTL_SECS))
}

fn path_lease_scan(path: &Path) -> crate::cargo_cache::CargoLeaseScan {
    crate::cargo_cache::scan_target_leases(path)
}

fn manager_cache_candidate_path(manager_id: &str) -> Option<PathBuf> {
    if manager_id
        .chars()
        .all(|ch| ch.is_ascii_hexdigit() || ch == '-')
    {
        Some(
            crate::config::cache_root_dir()
                .join("managers")
                .join(manager_id),
        )
    } else {
        None
    }
}

async fn remove_cache_paths_as_root(root: &Path, candidates: &[PathBuf]) -> Result<Vec<String>> {
    let container_paths = validated_cache_container_paths(root, candidates)?;
    if container_paths.is_empty() {
        return Ok(Vec::new());
    }
    let rel_paths: Vec<String> = container_paths
        .iter()
        .map(|path| path.trim_start_matches("/cache/").to_string())
        .collect();

    let mut command = tokio::process::Command::new("docker");
    command.args([
        "run",
        "--rm",
        "-v",
        &format!("{}:/cache:rw", root.display()),
        "alpine",
        "sh",
        "-eu",
        "-c",
        "for path do rm -rf -- \"$path\"; done",
        "jeryu-cache-gc",
    ]);
    command.args(&container_paths);
    let output = command
        .output()
        .await
        .context("removing manager caches through docker")?;

    if !output.status.success() {
        return Err(CacheError::CleanupFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
        .into());
    }
    Ok(rel_paths)
}

async fn remove_manager_cache_dirs_as_root(
    candidates: &[ManagerCacheStatus],
) -> Result<Vec<String>> {
    let paths: Vec<PathBuf> = candidates
        .iter()
        .filter_map(|cache| manager_cache_candidate_path(&cache.manager_id))
        .collect();
    remove_cache_paths_as_root(&crate::config::cache_root_dir(), &paths).await
}

async fn scan_cargo_target_dirs(root: &Path, scope: &str) -> Result<Vec<CargoTargetCacheStatus>> {
    let mut statuses = Vec::new();
    if !root.is_dir() {
        return Ok(statuses);
    }

    for entry in WalkDir::new(root).follow_links(false) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_dir() {
            continue;
        }
        if entry.file_name() != OsStr::new("target") {
            continue;
        }
        let path = entry.path().to_path_buf();
        let bytes = du_bytes(&path).await.unwrap_or(0);
        let age_seconds = path_age_seconds(&path);
        let lease_scan = path_lease_scan(&path);
        statuses.push(CargoTargetCacheStatus {
            scope: scope.to_string(),
            path: path.display().to_string(),
            bytes,
            active: lease_scan.active,
            lease_observed: lease_scan.observed_files > 0,
            age_seconds,
            gc_candidate: false,
            reason: if lease_scan.active {
                "active cargo target lease".to_string()
            } else if lease_scan.observed_files > 0 {
                "expired cargo target leases cleaned".to_string()
            } else {
                "cargo target cache".to_string()
            },
        });
        statuses.extend(scan_nextest_extract_dirs(&path, scope).await?);
    }

    statuses.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    Ok(statuses)
}

async fn scan_nextest_extract_dirs(
    target_dir: &Path,
    scope: &str,
) -> Result<Vec<CargoTargetCacheStatus>> {
    let extract_root = target_dir.join("nextest/extract");
    let mut statuses = Vec::new();
    if !extract_root.is_dir() {
        return Ok(statuses);
    }

    for entry in std::fs::read_dir(&extract_root)
        .with_context(|| format!("reading {}", extract_root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let bytes = du_bytes(&path).await.unwrap_or(0);
        let age_seconds = path_age_seconds(&path);
        let active = age_seconds
            .map(|age| age <= NEXTEST_EXTRACT_FALLBACK_TTL_SECS)
            .unwrap_or(true);
        let job_id = entry.file_name().to_string_lossy().to_string();
        statuses.push(CargoTargetCacheStatus {
            scope: format!("{scope}/nextest-extract:{job_id}"),
            path: path.display().to_string(),
            bytes,
            active,
            lease_observed: false,
            age_seconds,
            gc_candidate: false,
            reason: if active {
                "recent nextest extract scratch".to_string()
            } else {
                "expired nextest extract scratch".to_string()
            },
        });
    }

    statuses.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    Ok(statuses)
}

fn validated_cache_container_paths(root: &Path, candidates: &[PathBuf]) -> Result<Vec<String>> {
    let mut validated = Vec::new();
    for candidate in candidates {
        if !candidate.is_absolute() {
            anyhow::bail!(
                "refusing to remove non-absolute cache path: {}",
                candidate.display()
            );
        }
        let relative = candidate
            .strip_prefix(root)
            .with_context(|| format!("cache candidate outside root: {}", candidate.display()))?;
        let mut container_path = PathBuf::from("/cache");
        for component in relative.components() {
            match component {
                Component::Normal(segment) => container_path.push(segment),
                _ => {
                    anyhow::bail!(
                        "refusing to remove cache path with surprising components: {}",
                        candidate.display()
                    );
                }
            }
        }
        validated.push(container_path.display().to_string());
    }
    Ok(validated)
}

async fn gitlab_redis_write_check() -> HostDoctorCheck {
    let output = tokio::process::Command::new("docker")
        .args([
            "exec",
            "jeryu-gitlab",
            "sh",
            "-lc",
            "gitlab-redis-cli set jeryu:doctor:write ok EX 60 >/dev/null && gitlab-redis-cli get jeryu:doctor:write",
        ])
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => HostDoctorCheck {
            id: "gitlab-redis-write".to_string(),
            ok: String::from_utf8_lossy(&output.stdout).trim() == "ok",
            detail: "Redis accepts writes".to_string(),
        },
        Ok(output) => HostDoctorCheck {
            id: "gitlab-redis-write".to_string(),
            ok: false,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(err) => HostDoctorCheck {
            id: "gitlab-redis-write".to_string(),
            ok: false,
            detail: err.to_string(),
        },
    }
}

async fn gitlab_artifact_size_check() -> HostDoctorCheck {
    let env_file = crate::config::env_file();
    let script = format!(
        "set -a; . '{}'; set +a; curl -fsS -H \"PRIVATE-TOKEN: $GITLAB_PAT\" http://localhost:{}/api/v4/application/settings",
        env_file.display(),
        crate::config::GITLAB_HTTP_PORT
    );
    let output = tokio::process::Command::new("sh")
        .args(["-lc", &script])
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            let parsed = serde_json::from_slice::<serde_json::Value>(&output.stdout);
            match parsed.ok().and_then(|json| {
                json.get("max_artifacts_size")
                    .and_then(|value| value.as_u64())
            }) {
                Some(max_mb) => HostDoctorCheck {
                    id: "gitlab-artifact-size".to_string(),
                    ok: max_mb >= MIN_GITLAB_ARTIFACT_SIZE_MB,
                    detail: format!(
                        "max_artifacts_size={}MiB (required >= {}MiB)",
                        max_mb, MIN_GITLAB_ARTIFACT_SIZE_MB
                    ),
                },
                None => HostDoctorCheck {
                    id: "gitlab-artifact-size".to_string(),
                    ok: false,
                    detail: "could not parse max_artifacts_size".to_string(),
                },
            }
        }
        Ok(output) => HostDoctorCheck {
            id: "gitlab-artifact-size".to_string(),
            ok: false,
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(err) => HostDoctorCheck {
            id: "gitlab-artifact-size".to_string(),
            ok: false,
            detail: err.to_string(),
        },
    }
}

fn parse_age(raw: &str) -> Result<Duration> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CacheError::EmptyAge.into());
    }
    let (num, unit) = trimmed.split_at(trimmed.len() - 1);
    let value = num
        .parse::<u64>()
        .with_context(|| format!("invalid age value: {raw}"))?;
    match unit {
        "m" | "M" => Ok(Duration::from_secs(value * 60)),
        "h" | "H" => Ok(Duration::from_secs(value * 60 * 60)),
        "d" | "D" => Ok(Duration::from_secs(value * 24 * 60 * 60)),
        _ => Err(CacheError::UnsupportedAge(raw.to_string()).into()),
    }
}

fn gb_to_bytes(gb: f64) -> u64 {
    (gb * 1024.0 * 1024.0 * 1024.0) as u64
}

pub(crate) fn human_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= GIB {
        format!("{:.1}GiB", bytes as f64 / GIB)
    } else if bytes as f64 >= MIB {
        format!("{:.1}MiB", bytes as f64 / MIB)
    } else {
        format!("{}B", bytes)
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl CacheManager {
    pub async fn gc_disk_cache(&self) -> Result<()> {
        self.gc_disk_cache_with_pressure(false, false, false).await
    }

    pub async fn gc_disk_cache_with_pressure(
        &self,
        is_warning: bool,
        is_critical: bool,
        is_emergency: bool,
    ) -> Result<()> {
        // Pressure tiers are derived from free root bytes:
        // - warning: free space is below 80 GiB
        // - critical: free space is below 60 GiB
        // - emergency: free space is below 40 GiB
        // Automatic GC keeps active managers intact; the engine handles pausing and draining
        // the build/default pools before this path tries to reclaim inactive cache state.
        let (older_than, max_cache_gb, keep_active) = if is_emergency {
            ("15m".to_string(), Some(20.0_f64), true)
        } else if is_critical {
            ("2h".to_string(), Some(60.0_f64), true)
        } else if is_warning {
            ("4h".to_string(), Some(120.0_f64), true)
        } else {
            ("12h".to_string(), None, true)
        };

        SmartCache::new(crate::state::Db::open().await?)
            .gc_with_options(GcOptions {
                keep_active_managers: keep_active,
                older_than: Some(older_than),
                max_cache_gb,
                quiet: true,
                ..GcOptions::default()
            })
            .await
            .map(|_| ())
    }

    pub async fn status(&self) -> Result<()> {
        SmartCache::new(crate::state::Db::open().await?)
            .status()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};
    use tempfile::TempDir;

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

    #[tokio::test]
    async fn test_atomic_store_cas_success() -> Result<()> {
        let data = b"hello world";
        let digest = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

        SmartCache::atomic_store_cas(data, digest).await?;

        let cas_dir = crate::config::data_dir().join("cas");
        let path = cas_dir.join(digest);
        assert!(path.exists());

        // Cleanup explicitly
        let _ = tokio::fs::remove_file(path).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_atomic_store_cas_mismatch() {
        let data = b"hello world";
        let bad_digest = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

        let result = SmartCache::atomic_store_cas(data, bad_digest).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("mismatched hashes")
        );
    }

    #[test]
    fn gc_path_validation_accepts_literal_special_characters_and_rejects_parent_dirs() {
        let root = Path::new("/tmp/jeryu-cache");
        let good = vec![
            PathBuf::from("/tmp/jeryu-cache/cargo-targets/space dir/target"),
            PathBuf::from("/tmp/jeryu-cache/cargo-targets/quote'\";semi/target"),
            PathBuf::from("/tmp/jeryu-cache/cargo-targets/..literal/target"),
        ];
        let paths = validated_cache_container_paths(root, &good).unwrap();
        assert_eq!(
            paths,
            vec![
                "/cache/cargo-targets/space dir/target".to_string(),
                "/cache/cargo-targets/quote'\";semi/target".to_string(),
                "/cache/cargo-targets/..literal/target".to_string(),
            ]
        );

        let bad = vec![PathBuf::from(
            "/tmp/jeryu-cache/cargo-targets/../escape/target",
        )];
        assert!(validated_cache_container_paths(root, &bad).is_err());
    }

    #[test]
    fn pool_recovery_ttl_uses_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_env_var("JERYU_POOL_CARGO_LEASE_RECOVERY_SECS", "7");
        assert_eq!(pool_target_lease_recovery_ttl(), Duration::from_secs(7));
        remove_env_var("JERYU_POOL_CARGO_LEASE_RECOVERY_SECS");
    }

    #[test]
    fn root_disk_pressure_levels_follow_free_space_thresholds() {
        assert_eq!(
            root_disk_pressure_level(ROOT_DISK_WARNING_MIN_FREE_BYTES),
            DiskPressureLevel::Nominal
        );
        assert_eq!(
            root_disk_pressure_level(ROOT_DISK_WARNING_MIN_FREE_BYTES - 1),
            DiskPressureLevel::Warning
        );
        assert_eq!(
            root_disk_pressure_level(ROOT_DISK_CRITICAL_MIN_FREE_BYTES - 1),
            DiskPressureLevel::Critical
        );
        assert_eq!(
            root_disk_pressure_level(ROOT_DISK_EMERGENCY_MIN_FREE_BYTES - 1),
            DiskPressureLevel::Emergency
        );
    }

    #[tokio::test]
    async fn scan_cargo_target_dirs_marks_active_when_any_lease_is_live() -> Result<()> {
        let dir = TempDir::new()?;
        let target = dir
            .path()
            .join("cargo-targets")
            .join("scope")
            .join("target");
        std::fs::create_dir_all(&target)?;
        std::fs::write(target.join("artifact"), b"123")?;
        let lease_dir = target.join(crate::cargo_cache::LEASES_DIR_NAME);
        std::fs::create_dir_all(&lease_dir)?;

        let expired = crate::cargo_cache::CargoLeaseRecord {
            kind: "local-cargo".to_string(),
            scope_key: "scope".to_string(),
            target_dir: target.display().to_string(),
            pid: u32::MAX,
            created_at: chrono::Utc::now().to_rfc3339(),
            rustc_key: "rustc".to_string(),
            rustc_version: "rustc".to_string(),
            host_triple: "host".to_string(),
        };
        let active = crate::cargo_cache::CargoLeaseRecord {
            pid: std::process::id(),
            ..expired.clone()
        };
        std::fs::write(
            lease_dir.join("expired.json"),
            serde_json::to_vec_pretty(&expired)?,
        )?;
        std::fs::write(
            lease_dir.join("active.json"),
            serde_json::to_vec_pretty(&active)?,
        )?;

        let statuses = scan_cargo_target_dirs(&dir.path().join("cargo-targets"), "local").await?;
        assert_eq!(statuses.len(), 1);
        assert!(statuses[0].active);
        assert!(statuses[0].lease_observed);
        assert!(lease_dir.join("active.json").exists());
        assert!(!lease_dir.join("expired.json").exists());
        Ok(())
    }

    #[tokio::test]
    async fn scan_cargo_target_dirs_reports_nested_nextest_extract_scratch() -> Result<()> {
        let dir = TempDir::new()?;
        let target = dir
            .path()
            .join("cargo-targets")
            .join("scope")
            .join("target");
        let nested_extract = target
            .join("nextest")
            .join("extract")
            .join("test-rust-nextest-1");
        std::fs::create_dir_all(&nested_extract)?;
        std::fs::write(nested_extract.join("artifact"), b"nextest")?;

        let statuses = scan_cargo_target_dirs(&dir.path().join("cargo-targets"), "local").await?;
        assert_eq!(statuses.len(), 2);
        assert!(
            statuses
                .iter()
                .any(|status| status.scope == "local" && status.path.ends_with("/target"))
        );
        assert!(statuses.iter().any(|status| {
            status.scope == "local/nextest-extract:test-rust-nextest-1"
                && status.path.ends_with("nextest/extract/test-rust-nextest-1")
        }));
        Ok(())
    }
}
