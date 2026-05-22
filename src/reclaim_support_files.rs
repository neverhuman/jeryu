use super::*;
use tracing::warn;

pub(crate) async fn evict_artifacts_over_budget(
    dir: &Path,
    suffix: &str,
    budget_bytes: u64,
) -> u64 {
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    walk_files_with_suffix(dir, suffix, |path, meta| {
        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        files.push((path, meta.len(), mtime));
    })
    .await;

    let total: u64 = files.iter().map(|(_, sz, _)| sz).sum();
    if total <= budget_bytes {
        return 0;
    }

    files.sort_by_key(|(_, _, mtime)| *mtime);
    let mut to_free = total - budget_bytes;
    let mut removed = 0u64;
    for (path, size, _) in files {
        if to_free == 0 {
            break;
        }
        if let Err(e) = tokio::fs::remove_file(&path).await {
            warn!(path = %path.display(), error = %e, "artifact eviction failed");
        } else {
            to_free = to_free.saturating_sub(size);
            removed += 1;
        }
    }
    removed
}

pub(crate) async fn sweep_stale_files(
    dir: &Path,
    suffix: &str,
    max_age: std::time::Duration,
) -> u64 {
    let mut removed = 0u64;
    let mut to_remove: Vec<PathBuf> = Vec::new();
    walk_files_with_suffix(dir, suffix, |path, meta| {
        let is_stale = meta
            .modified()
            .ok()
            .and_then(|mtime| std::time::SystemTime::now().duration_since(mtime).ok())
            .is_some_and(|age| age >= max_age);
        if is_stale {
            to_remove.push(path);
        }
    })
    .await;

    for path in to_remove {
        if let Err(e) = tokio::fs::remove_file(&path).await {
            warn!(path = %path.display(), error = %e, "failed to remove outdated artifact");
        } else {
            removed += 1;
        }
    }
    removed
}

async fn walk_files_with_suffix<F>(dir: &Path, suffix: &str, mut visit: F)
where
    F: FnMut(PathBuf, std::fs::Metadata),
{
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
                continue;
            }
            let name = entry.file_name();
            if !name.to_string_lossy().ends_with(suffix) {
                continue;
            }
            visit(entry.path(), meta);
        }
    }
}
