//! Root-owned compiler and Cargo configuration with component projections.

use anyhow::{Context, Result, bail, ensure};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use toml::Value;

struct Projection {
    path: PathBuf,
    original: Vec<u8>,
    desired: Vec<u8>,
    permissions: fs::Permissions,
}

fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("cannot read build configuration {}", path.display()))?;
    ensure!(
        metadata.is_file() && path.canonicalize()? == path,
        "build configuration must be a physical regular file: {}",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.nlink() == 1,
            "build configuration has multiple links: {}",
            path.display()
        );
    }
    fs::read(path).with_context(|| format!("cannot read {}", path.display()))
}

fn toml_file(path: &Path) -> Result<(Vec<u8>, Value)> {
    let bytes = read_regular(path)?;
    let value = toml::from_str(std::str::from_utf8(&bytes)?)
        .with_context(|| format!("invalid build configuration TOML: {}", path.display()))?;
    Ok((bytes, value))
}

fn reject_legacy_overrides(directory: &Path) -> Result<()> {
    for name in ["rust-toolchain", ".cargo/config"] {
        let path = directory.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => bail!(
                "legacy build configuration overrides root authority: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn projections(root: &Path) -> Result<Vec<Projection>> {
    reject_legacy_overrides(root)?;
    let (toolchain, pin) = toml_file(&root.join("rust-toolchain.toml"))?;
    let channel = pin
        .get("toolchain")
        .and_then(|t| t.get("channel"))
        .and_then(Value::as_str)
        .context("root Rust toolchain channel is missing")?;
    let parts: Vec<_> = channel.split('.').collect();
    ensure!(
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "root Rust toolchain must pin MAJOR.MINOR.PATCH"
    );
    let (cargo_config, _) = toml_file(&root.join(".cargo/config.toml"))?;
    let (_, manifest) = toml_file(&root.join("repos.manifest.toml"))?;
    crate::monorepo::validate_manifest(&manifest, false)?;
    let repositories = manifest["repo"]
        .as_array()
        .context("repository inventory")?;
    ensure!(
        repositories
            .iter()
            .any(|repo| repo["name"].as_str() == Some("jeryu")),
        "repository inventory must contain the monorepo root"
    );
    let mut result = Vec::new();
    for repo in repositories {
        let name = repo["name"].as_str().context("component name")?;
        if name == "jeryu" {
            continue;
        }
        ensure!(
            name.starts_with("jeryu-") && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'),
            "invalid component name: {name}"
        );
        let component = root.join(repo["path"].as_str().context("component path")?);
        ensure!(
            component.is_dir() && component.canonicalize()? == component,
            "component must be a physical directory: {}",
            component.display()
        );
        reject_legacy_overrides(&component)?;
        let mut paths = vec![(component.join("rust-toolchain.toml"), &toolchain)];
        let local_config = component.join(".cargo/config.toml");
        match fs::symlink_metadata(&local_config) {
            Ok(_) => paths.push((local_config, &cargo_config)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // No component override means Cargo inherits the root config.
                // An existing .cargo directory must still be physical.
                let directory = component.join(".cargo");
                match fs::symlink_metadata(&directory) {
                    Ok(metadata) => ensure!(
                        metadata.is_dir() && directory.canonicalize()? == directory,
                        "component Cargo directory must be physical: {}",
                        directory.display()
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
        for (path, desired) in paths {
            let original = read_regular(&path)?;
            let permissions = fs::metadata(&path)?.permissions();
            result.push(Projection {
                path,
                original,
                desired: desired.clone(),
                permissions,
            });
        }
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

/// Check the projections, or explicitly refresh existing generated files.
pub(super) fn run(root: &Path, write: bool) -> Result<()> {
    let root = root.canonicalize().context("monorepo root")?;
    // Complete path/input admission precedes the first requested write.
    let planned = projections(&root)?;
    let mut changed = 0;
    for projection in &planned {
        if projection.original == projection.desired {
            continue;
        }
        ensure!(
            write,
            "component build configuration drift: {}; run jeryu-split build-config --write from the monorepo root",
            projection.path.display()
        );
        ensure!(
            read_regular(&projection.path)? == projection.original,
            "build configuration changed during generation: {}",
            projection.path.display()
        );
        let parent = projection.path.parent().context("projection parent")?;
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        output
            .as_file()
            .set_permissions(projection.permissions.clone())?;
        output.write_all(&projection.desired)?;
        output.as_file().sync_all()?;
        output.persist(&projection.path)?;
        changed += 1;
    }
    if write {
        ensure!(
            projections(&root)?.iter().all(|p| p.original == p.desired),
            "root build configuration or component projections changed during generation"
        );
    }
    println!(
        "{} component build configuration projections match root authority; {changed} refreshed",
        planned.len()
    );
    Ok(())
}

#[cfg(test)]
#[path = "build_config_tests.rs"]
mod tests;
