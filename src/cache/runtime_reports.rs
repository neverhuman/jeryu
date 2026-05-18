use anyhow::Result;
use std::collections::BTreeSet;

use super::*;

impl SmartCache {
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
        let mut manager_cargo_targets = Vec::new();
        let mut manager_cargo_target_bytes = 0_u64;
        let mut manager_cargo_sccache_bytes = 0_u64;

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
                manager_cargo_sccache_bytes =
                    manager_cargo_sccache_bytes.saturating_add(sccache_bytes);
                let mut statuses = scan_cargo_target_dirs(
                    &path.join("cargo-targets"),
                    &format!("manager:{manager_id}"),
                )
                .await?;
                if active {
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
                                "active manager cargo cache recovery path (lease absent)"
                                    .to_string();
                        } else if !status.lease_observed {
                            status.reason = "manager cargo cache without lease".to_string();
                        }
                    }
                }
                manager_cargo_target_bytes = manager_cargo_target_bytes
                    .saturating_add(statuses.iter().map(|status| status.bytes).sum::<u64>());
                manager_cargo_targets.extend(statuses);
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
        cargo_target_caches.extend(manager_cargo_targets);
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
            manager_cargo_target_bytes,
            manager_cargo_sccache_bytes,
            local_cargo_target_bytes,
            local_cargo_sccache_bytes,
            pool_cargo_target_bytes,
            pool_cargo_sccache_bytes,
            cargo_target_caches,
            docker: match docker_storage_summary().await {
                Ok(d) => d,
                Err(_) => DockerStorageSummary::default(),
            },
            proxy_up,
            registry_up,
        })
    }
}
