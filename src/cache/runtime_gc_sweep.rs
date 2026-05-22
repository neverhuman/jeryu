use anyhow::Result;
use chrono::Duration as ChronoDuration;

use super::*;

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
#[path = "runtime_gc_tests.rs"]
mod sweep_tests;
