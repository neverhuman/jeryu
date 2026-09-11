//! Check every active manifest before Cargo resolves its local source paths.
//! Preserves Deploy PR 1's path-alias guards without its historical split pins.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};
use toml::Value;

fn local_directory(root: &Path, directory: &Path, selected: &str) -> Result<PathBuf> {
    ensure!(!selected.is_empty(), "empty dependency path");
    let mut resolved = directory.to_path_buf();
    for component in Path::new(selected).components() {
        match component {
            Component::Normal(name) => resolved.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                ensure!(resolved != root, "dependency path leaves this repository");
                ensure!(resolved.pop(), "dependency path leaves this repository");
            }
            _ => anyhow::bail!("dependency path must be repository-relative"),
        }
    }
    ensure!(
        resolved.starts_with(root)
            && resolved.is_dir()
            && resolved
                .canonicalize()
                .context("dependency path unavailable")?
                == resolved,
        "dependency path must be an existing physical directory in this repository"
    );
    Ok(resolved)
}

fn read_manifest(path: &Path) -> Result<Value> {
    let metadata = fs::symlink_metadata(path).context("active manifest unavailable")?;
    ensure!(
        metadata.is_file() && path.canonicalize()? == path,
        "active manifest must be a physical regular file"
    );
    let bytes = fs::read_to_string(path).context("read active manifest")?;
    toml::from_str(&bytes).context("parse active manifest")
}

fn dependency_section(root: &Path, directory: &Path, section: Option<&Value>) -> Result<()> {
    let Some(section) = section else {
        return Ok(());
    };
    for (name, declaration) in section
        .as_table()
        .context("dependency section must be a table")?
    {
        if let Some(path) = declaration.as_table().and_then(|value| value.get("path")) {
            let path = path.as_str().context("dependency path must be a string")?;
            local_directory(root, directory, path)
                .with_context(|| format!("{name}: invalid dependency path"))?;
        }
    }
    Ok(())
}

fn manifest_paths(root: &Path, directory: &Path, manifest: &Value) -> Result<()> {
    for key in [
        "dependencies",
        "dev-dependencies",
        "build-dependencies",
        "replace",
    ] {
        dependency_section(root, directory, manifest.get(key))?;
    }
    if let Some(workspace) = manifest.get("workspace") {
        let workspace = workspace.as_table().context("workspace must be a table")?;
        dependency_section(root, directory, workspace.get("dependencies"))?;
    }
    if let Some(targets) = manifest.get("target") {
        for target in targets
            .as_table()
            .context("target sections must be tables")?
            .values()
        {
            let target = target.as_table().context("target entry must be a table")?;
            for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                dependency_section(root, directory, target.get(key))?;
            }
        }
    }
    if let Some(patches) = manifest.get("patch") {
        for section in patches
            .as_table()
            .context("patch sections must be tables")?
            .values()
        {
            dependency_section(root, directory, Some(section))?;
        }
    }
    if let Some(workspace) = manifest
        .get("package")
        .and_then(|value| value.get("workspace"))
    {
        let workspace = workspace
            .as_str()
            .context("package workspace must be a path")?;
        ensure!(
            local_directory(root, directory, workspace)? == root,
            "package selects another workspace"
        );
    }
    Ok(())
}

pub(super) fn check(root: &Path) -> Result<()> {
    ensure!(
        root.is_absolute() && root.canonicalize()? == root,
        "manifest source root must be physical and absolute"
    );
    let manifest = read_manifest(&root.join("Cargo.toml"))?;
    manifest_paths(root, root, &manifest)?;
    let members = manifest
        .get("workspace")
        .and_then(|value| value.get("members"))
        .and_then(Value::as_array)
        .context("explicit workspace members required")?;
    let mut seen = BTreeSet::new();
    for member in members {
        let member = member.as_str().context("workspace member must be a path")?;
        ensure!(
            !member.contains(['*', '?', '[']),
            "explicit workspace member paths required"
        );
        let directory = local_directory(root, root, member)?;
        ensure!(
            seen.insert(directory.clone()),
            "duplicate workspace member path"
        );
        let manifest = read_manifest(&directory.join("Cargo.toml"))?;
        manifest_paths(root, &directory, &manifest)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "monorepo_paths_tests.rs"]
mod tests;
