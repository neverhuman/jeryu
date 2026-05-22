use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use super::display_relative;

pub fn context_metrics(workspace_root: &Path, package_root: &Path) -> Result<(usize, u64)> {
    let mut file_count = 0usize;
    let mut bytes = 0u64;
    for entry in WalkDir::new(package_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        if !matches!(extension, "rs" | "toml" | "md" | "json" | "yaml" | "yml") {
            continue;
        }
        file_count += 1;
        bytes += fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?
            .len();
    }
    let root_display = display_relative(workspace_root, package_root);
    if file_count == 0 {
        return Ok((0, 0));
    }
    let _ = root_display;
    Ok((file_count, bytes))
}
