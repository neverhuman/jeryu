use anyhow::Result;
use chrono::Duration as ChronoDuration;

use super::*;

impl SmartCache {
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
        let mut removed = Vec::new();
        let mut errors = Vec::new();
        let mut removed_cargo = Vec::new();

        if !options.dry_run && !candidates.is_empty() {
            match remove_manager_cache_dirs_as_root(&candidates).await {
                Ok(r) => removed = r,
                Err(err) => errors.push(err.to_string()),
            }
        }
        if !options.dry_run && !cargo_candidates.is_empty() {
            let paths: Vec<PathBuf> = cargo_candidates
                .iter()
                .map(|cache| PathBuf::from(&cache.path))
                .collect();
            match remove_cache_paths_as_root(&crate::config::cache_root_dir(), &paths).await {
                Ok(r) => removed_cargo = r,
                Err(err) => errors.push(err.to_string()),
            }
        }

        let gc_slots_freed = self.run_gc_housekeeping(options.dry_run).await?;
        let report = CacheGcReport {
            dry_run: options.dry_run,
            removed_manager_caches: removed,
            candidate_manager_caches: candidates,
            removed_cargo_targets: removed_cargo,
            candidate_cargo_targets: cargo_candidates,
            gc_eviction_count: gc_slots_freed,
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
}

/// Walk every `target/.../incremental/` directory under the JeRyu cache root
/// and remove entries based on the current disk-pressure level. Active leases
/// are preserved at every tier below Emergency. Returns bytes freed.
///
/// At `Emergency`, additionally sweeps `target/debug/incremental/` in the
/// **workspace** root if `JERYU_GCD_ALLOW_LOCAL_TARGET_SWEEP=1` is set.
///
/// Called by `jeryu-gcd` from the EmergencyGc tick action.
pub async fn sweep_incremental_caches(pressure: DiskPressureLevel) -> Result<u64> {
    use crate::config as config_paths;
    let mut freed: u64 = 0;
    let roots = candidate_incremental_roots();
    let age_floor = match pressure {
        DiskPressureLevel::Nominal => return Ok(0),
        DiskPressureLevel::Warning => Some(ChronoDuration::minutes(30)),
        DiskPressureLevel::Critical => Some(ChronoDuration::minutes(0)),
        DiskPressureLevel::Emergency => None,
    };
    for root in &roots {
        if !root.exists() {
            continue;
        }
        freed += sweep_one_incremental_root(root, age_floor).await;
    }
    // Workspace local target sweep is opt-in even under Emergency to protect
    // active developer work.
    if matches!(pressure, DiskPressureLevel::Emergency)
        && std::env::var("JERYU_GCD_ALLOW_LOCAL_TARGET_SWEEP").as_deref() == Ok("1")
    {
        let cwd = match std::env::current_dir() {
            Ok(d) => d,
            Err(_) => PathBuf::from("."),
        };
        let workspace_incremental = cwd.join("target").join("debug").join("incremental");
        if workspace_incremental.exists() {
            freed += sweep_one_incremental_root(&workspace_incremental, None).await;
        }
    }
    // Ensure the config helper is referenced regardless of the incremental-sweep
    // env flag, keeping the import valid in all build configurations.
    let _ = config_paths::cache_root_dir();
    Ok(freed)
}

fn candidate_incremental_roots() -> Vec<PathBuf> {
    use crate::config as config_paths;
    let mut out = Vec::new();
    // Local cargo target dirs JeRyu manages on this host.
    out.push(config_paths::local_cargo_targets_root());
    // Pool-shared cargo target dirs (one per pool, name discovered on disk).
    let pools_root = config_paths::cache_root_dir().join("pools");
    if pools_root.exists()
        && let Ok(entries) = std::fs::read_dir(&pools_root)
    {
        for entry in entries.flatten() {
            let p = entry.path().join("cargo-targets");
            if p.exists() {
                out.push(p);
            }
        }
    }
    out
}

async fn sweep_one_incremental_root(root: &Path, age_floor: Option<ChronoDuration>) -> u64 {
    let mut freed: u64 = 0;
    let cutoff = age_floor.map(|d| SystemTime::now() - Duration::from_secs(d.num_seconds() as u64));
    // We look for any directory whose path component is "incremental".
    for entry in WalkDir::new(root)
        .min_depth(1)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| e.file_name() != OsStr::new(".jeryu"))
        .flatten()
    {
        if entry.file_name() != OsStr::new("incremental") || !entry.file_type().is_dir() {
            continue;
        }
        // Skip if a sibling .jeryu/leases/*.json exists (active lease on the
        // parent target directory).
        if has_active_lease(entry.path()) {
            continue;
        }
        if let Some(min_time) = cutoff {
            match std::fs::metadata(entry.path()).and_then(|m| m.modified()) {
                Ok(mtime) if mtime > min_time => continue,
                _ => {}
            }
        }
        let bytes = dir_size_bytes(entry.path());
        if let Err(err) = std::fs::remove_dir_all(entry.path()) {
            info!(path = %entry.path().display(), error = %err, "sweep_incremental remove failed");
            continue;
        }
        freed += bytes;
        info!(path = %entry.path().display(), freed_bytes = bytes, "swept incremental");
    }
    freed
}

fn has_active_lease(incremental_dir: &Path) -> bool {
    // The parent of an "incremental" dir is normally the cargo profile dir
    // (e.g. target/debug). The grandparent is the target dir whose
    // .jeryu/leases/*.json file marks an active lease.
    let Some(profile_dir) = incremental_dir.parent() else {
        return false;
    };
    let Some(target_dir) = profile_dir.parent() else {
        return false;
    };
    let leases = target_dir.join(".jeryu").join("leases");
    if !leases.is_dir() {
        return false;
    }
    match std::fs::read_dir(&leases) {
        Ok(mut it) => it.next().is_some(),
        Err(_) => false,
    }
}

fn dir_size_bytes(path: &Path) -> u64 {
    let mut total: u64 = 0;
    for entry in WalkDir::new(path).into_iter().flatten() {
        if entry.file_type().is_file()
            && let Ok(m) = entry.metadata()
        {
            total += m.len();
        }
    }
    total
}

#[cfg(test)]
mod sweep_tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn nominal_pressure_returns_zero_without_touching_anything() {
        let freed = sweep_incremental_caches(DiskPressureLevel::Nominal)
            .await
            .unwrap();
        assert_eq!(freed, 0);
    }

    #[test]
    fn dir_size_counts_files() {
        let tmp = tempdir().unwrap();
        let f = tmp.path().join("a.bin");
        std::fs::write(&f, vec![0u8; 1024]).unwrap();
        assert!(dir_size_bytes(tmp.path()) >= 1024);
    }

    #[test]
    fn has_active_lease_true_when_leases_present() {
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("target");
        let profile = target.join("debug");
        let incremental = profile.join("incremental");
        std::fs::create_dir_all(&incremental).unwrap();
        let leases = target.join(".jeryu").join("leases");
        std::fs::create_dir_all(&leases).unwrap();
        std::fs::write(leases.join("x.json"), "{}").unwrap();
        assert!(has_active_lease(&incremental));
    }

    #[test]
    fn has_active_lease_false_when_no_leases() {
        let tmp = tempdir().unwrap();
        let inc = tmp.path().join("target").join("debug").join("incremental");
        std::fs::create_dir_all(&inc).unwrap();
        assert!(!has_active_lease(&inc));
    }
}
