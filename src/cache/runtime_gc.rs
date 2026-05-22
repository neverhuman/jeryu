use anyhow::Result;

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
            + status.manager_cargo_target_bytes
            + status.local_cargo_target_bytes
            + status.pool_cargo_target_bytes
            + status.manager_cargo_sccache_bytes
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

#[path = "runtime_gc_sweep.rs"]
mod sweep;
pub use sweep::sweep_incremental_caches;
