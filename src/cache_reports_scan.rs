use super::*;

const NEXTEST_EXTRACT_RECENT_TTL_SECS: u64 = 2 * 60 * 60;

pub(crate) async fn remove_cache_paths_as_root(
    root: &Path,
    candidates: &[PathBuf],
) -> Result<Vec<String>> {
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

pub(crate) async fn remove_manager_cache_dirs_as_root(
    candidates: &[ManagerCacheStatus],
) -> Result<Vec<String>> {
    let paths: Vec<PathBuf> = candidates
        .iter()
        .filter_map(|cache| manager_cache_candidate_path(&cache.manager_id))
        .collect();
    remove_cache_paths_as_root(&crate::config::cache_root_dir(), &paths).await
}

pub(crate) async fn scan_cargo_target_dirs(
    root: &Path,
    scope: &str,
) -> Result<Vec<CargoTargetCacheStatus>> {
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

pub(crate) async fn scan_nextest_extract_dirs(
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
            .map(|age| age <= NEXTEST_EXTRACT_RECENT_TTL_SECS)
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

pub(crate) fn count_named_marker_files(root: &Path, marker_dir: &str) -> Result<u64> {
    if !root.is_dir() {
        return Ok(0);
    }

    let mut count = 0_u64;
    for entry in WalkDir::new(root).follow_links(false) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .path()
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            != Some(marker_dir)
        {
            continue;
        }
        if entry.path().parent().and_then(|parent| parent.parent()) != Some(root) {
            continue;
        }
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_named_marker_files_only_counts_direct_children() {
        let dir = tempfile::TempDir::new().unwrap();
        let marker_dir = dir.path().join(".jeryu-cache-seeds");
        std::fs::create_dir_all(&marker_dir).unwrap();
        std::fs::write(marker_dir.join("one.json"), b"{}").unwrap();
        std::fs::write(marker_dir.join("two.json"), b"{}").unwrap();
        std::fs::create_dir_all(dir.path().join("nested/.jeryu-cache-seeds")).unwrap();
        std::fs::write(
            dir.path().join("nested/.jeryu-cache-seeds/ignored.json"),
            b"{}",
        )
        .unwrap();

        let count = count_named_marker_files(dir.path(), ".jeryu-cache-seeds").unwrap();
        assert_eq!(count, 2);
    }
}
