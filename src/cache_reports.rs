use super::*;

const POOL_TARGET_LEASE_RECOVERY_TTL_SECS: u64 = 2 * 60 * 60;

#[path = "cache_reports_manager.rs"]
mod manager;
#[path = "cache_reports_scan.rs"]
mod scan;

pub(crate) use manager::*;
pub(crate) use scan::*;

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
    if !report.removed_manager_caches.is_empty() {
        println!("Removed: {}", report.removed_manager_caches.len());
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
    if !report.removed_cargo_targets.is_empty() {
        println!("Cargo removed: {}", report.removed_cargo_targets.len());
    }
    if report.gc_eviction_count > 0 {
        println!("GC evictions: {}", report.gc_eviction_count);
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

pub(crate) async fn tcp_up(port: u16) -> bool {
    tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .is_ok()
}

pub(crate) async fn active_runner_manager_ids() -> BTreeSet<String> {
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

pub(crate) async fn du_bytes(path: &Path) -> Result<u64> {
    let output = tokio::process::Command::new("du")
        .args(["-sb", &path.display().to_string()])
        .output()
        .await
        .with_context(|| format!("du -sb {}", path.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = stdout
        .split_whitespace()
        .next()
        .and_then(|first| first.parse::<u64>().ok())
        .filter(|value| *value > 0);
    if let Some(bytes) = parsed {
        return Ok(bytes);
    }

    let mut total = 0_u64;
    if path.is_file() {
        total = path.metadata().map(|meta| meta.len()).unwrap_or(0);
    } else if path.is_dir() {
        for entry in WalkDir::new(path).follow_links(false) {
            let Ok(entry) = entry else {
                continue;
            };
            if entry.file_type().is_file() {
                total = total.saturating_add(entry.metadata().map(|meta| meta.len()).unwrap_or(0));
            }
        }
    }
    Ok(total)
}

/// Run `df -Pk <path>` and parse the result. Public so `jeryu-gcd` can
/// query disk usage without duplicating parsing logic.
pub async fn df_usage(path: &str) -> Result<FsUsage> {
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
    let line = match stdout.lines().nth(1) {
        Some(l) => l,
        None => anyhow::bail!("df output missing data row"),
    };
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

pub(crate) async fn docker_storage_summary() -> Result<DockerStorageSummary> {
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

pub(crate) fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub(crate) fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|v| match v.as_u64() {
        Some(n) => Some(n),
        None => v.as_str().and_then(|s| s.parse::<u64>().ok()),
    })
}

pub(crate) fn path_age_seconds(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|duration| duration.as_secs())
}

pub(crate) fn pool_target_lease_recovery_ttl() -> Duration {
    std::env::var("JERYU_POOL_CARGO_LEASE_RECOVERY_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(POOL_TARGET_LEASE_RECOVERY_TTL_SECS))
}

pub(crate) fn path_lease_scan(path: &Path) -> crate::cargo_cache::CargoLeaseScan {
    crate::cargo_cache::scan_target_leases(path)
}

pub(crate) fn manager_cache_candidate_path(manager_id: &str) -> Option<PathBuf> {
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

pub(crate) fn validated_cache_container_paths(
    root: &Path,
    candidates: &[PathBuf],
) -> Result<Vec<String>> {
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
