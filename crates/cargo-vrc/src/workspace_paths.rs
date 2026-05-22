use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub(crate) fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_root = manifest_dir.parent().context("workspace crate root")?;
    let root = crate_root.parent().context("workspace root")?;
    Ok(root.to_path_buf())
}

pub(crate) fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize path {}", path.display()))
}

pub(crate) fn normalize_manifest_path(path: &Path) -> Result<PathBuf> {
    let normalized = normalize_existing_path(path)?;
    let root = workspace_root()?;
    if !normalized.starts_with(&root) {
        anyhow::bail!(
            "manifest path {} escapes workspace root {}",
            normalized.display(),
            root.display()
        );
    }
    if normalized.file_name().and_then(|name| name.to_str()) != Some("Cargo.toml") {
        anyhow::bail!("workspace manifest path must point to Cargo.toml");
    }
    Ok(normalized)
}

pub(crate) fn normalize_workspace_path(root: &Path, path: &Path) -> Result<PathBuf> {
    let normalized_path = normalize_existing_path(path)?;
    if normalized_path.starts_with(root) {
        Ok(normalized_path)
    } else {
        anyhow::bail!(
            "workspace path {} escapes workspace root {}",
            normalized_path.display(),
            root.display()
        );
    }
}

pub(crate) fn display_relative(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root).ok() {
        Some(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ if path == root => ".".to_string(),
        _ => path.display().to_string(),
    }
}

pub(crate) fn sorted_lookup(
    map: &std::collections::HashMap<String, Vec<String>>,
    key: &str,
) -> Vec<String> {
    let mut values = match map.get(key) {
        Some(values) => values.clone(),
        None => Vec::new(),
    };
    values.sort();
    values.dedup();
    values
}
