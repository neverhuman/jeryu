use super::*;

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
        "Cargo targets: local={} manager={} pool={}",
        human_bytes(report.local_cargo_target_bytes),
        human_bytes(report.manager_cargo_target_bytes),
        human_bytes(report.pool_cargo_target_bytes)
    );
    println!(
        "Sccache dirs:  local={} manager={} pool={}",
        human_bytes(report.local_cargo_sccache_bytes),
        human_bytes(report.manager_cargo_sccache_bytes),
        human_bytes(report.pool_cargo_sccache_bytes)
    );
    println!(
        "Cargo homes:   local={} manager={} pool={}",
        human_bytes(report.local_cargo_home_bytes),
        human_bytes(report.manager_cargo_home_bytes),
        human_bytes(report.pool_cargo_home_bytes)
    );
    println!(
        "Rustup homes:  local={} manager={} pool={}",
        human_bytes(report.local_rustup_home_bytes),
        human_bytes(report.manager_rustup_home_bytes),
        human_bytes(report.pool_rustup_home_bytes)
    );
    println!(
        "Target warm markers: seeds={} promotes={}",
        report.target_seed_count, report.target_promote_count
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
