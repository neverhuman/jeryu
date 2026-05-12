use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::{Metadata, MetadataCommand, Package};

use crate::model::{PackageAgentMetadata, WorkspaceAgentMetadata};

#[derive(Debug, Clone)]
pub struct PackageSnapshot {
    pub name: String,
    pub manifest_path: PathBuf,
    pub package_root: PathBuf,
    pub agent: PackageAgentMetadata,
    pub direct_dependencies: Vec<String>,
    pub reverse_dependencies: Vec<String>,
    pub target_names: Vec<String>,
    pub target_tests: Vec<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    pub metadata: Metadata,
    pub workspace_root: PathBuf,
    pub workspace_agent: WorkspaceAgentMetadata,
    pub packages: Vec<PackageSnapshot>,
}

/// Load a snapshot of the cargo workspace.
///
/// Security boundary (HLT-023-INPUT-BOUNDARY-GAP): the only externally supplied
/// input is `manifest_path`, which is filtered through [`normalize_manifest_path`]
/// before reaching `cargo_metadata`. Normalization canonicalizes the path,
/// requires the canonical form to live under the compile-time `workspace_root()`
/// allowlist, and rejects any filename other than `Cargo.toml`. The downstream
/// call below is `cargo_metadata::MetadataCommand::exec`, which spawns
/// `cargo metadata` via `std::process::Command` with structured arguments — no
/// shell interpretation occurs. Negative coverage lives in the unit tests
/// `normalize_manifest_path_*` below.
pub fn load_workspace(manifest_path: Option<&Path>) -> Result<WorkspaceSnapshot> {
    let mut metadata_query = MetadataCommand::new();
    if let Some(path) = manifest_path {
        metadata_query.manifest_path(&normalize_manifest_path(path)?);
    } else {
        metadata_query.manifest_path(workspace_root()?.join("Cargo.toml"));
    }
    // Structured subprocess invocation through `cargo_metadata::MetadataCommand`
    // (not a shell string); the manifest path is path-validated above.
    let metadata = metadata_query
        .exec() // allowlist: structured cargo_metadata invocation, path-validated manifest
        .context("failed to read cargo metadata (see normalize_manifest_path)")?;
    let workspace_root = normalize_existing_path(metadata.workspace_root.as_std_path())?;
    let workspace_agent = parse_workspace_agent(&metadata.workspace_metadata)?;
    let member_ids: HashSet<_> = metadata.workspace_members.iter().cloned().collect();
    let package_by_id: HashMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect();

    let mut direct: HashMap<String, Vec<String>> = HashMap::new();
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(resolve) = &metadata.resolve {
        for node in &resolve.nodes {
            if !member_ids.contains(&node.id) {
                continue;
            }
            let Some(package) = package_by_id.get(&node.id) else {
                continue;
            };
            for dep in &node.deps {
                if !member_ids.contains(&dep.pkg) {
                    continue;
                }
                if let Some(dep_package) = package_by_id.get(&dep.pkg) {
                    direct
                        .entry(package.name.to_string())
                        .or_default()
                        .push(dep_package.name.to_string());
                    reverse
                        .entry(dep_package.name.to_string())
                        .or_default()
                        .push(package.name.to_string());
                }
            }
        }
    }

    let packages = metadata
        .packages
        .iter()
        .filter(|package| member_ids.contains(&package.id))
        .map(|package| package_snapshot(package, &workspace_root, &direct, &reverse))
        .collect::<Result<Vec<_>>>()?;

    Ok(WorkspaceSnapshot {
        metadata,
        workspace_root,
        workspace_agent,
        packages,
    })
}

fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_root = manifest_dir.parent().context("workspace crate root")?;
    let root = crate_root.parent().context("workspace root")?;
    Ok(root.to_path_buf())
}

fn parse_workspace_agent(value: &serde_json::Value) -> Result<WorkspaceAgentMetadata> {
    match value.get("agent").cloned() {
        Some(agent) if !agent.is_null() => {
            serde_json::from_value(agent).context("failed to parse workspace.metadata.agent")
        }
        _ => Ok(WorkspaceAgentMetadata::default()),
    }
}

fn parse_package_agent(package: &Package) -> Result<PackageAgentMetadata> {
    match package.metadata.get("agent").cloned() {
        Some(agent) if !agent.is_null() => serde_json::from_value(agent).with_context(|| {
            format!(
                "failed to parse package.metadata.agent for {}",
                package.name
            )
        }),
        _ => Ok(PackageAgentMetadata::default()),
    }
}

fn package_snapshot(
    package: &Package,
    workspace_root: &Path,
    direct: &HashMap<String, Vec<String>>,
    reverse: &HashMap<String, Vec<String>>,
) -> Result<PackageSnapshot> {
    let manifest_path =
        normalize_workspace_path(workspace_root, package.manifest_path.as_std_path())?;
    let package_root = manifest_path
        .parent()
        .context("package manifest unexpectedly missing parent directory")?
        .to_path_buf();
    let agent = parse_package_agent(package)?;
    let mut target_names = package
        .targets
        .iter()
        .map(|target| target.name.clone())
        .collect::<Vec<_>>();
    target_names.sort();
    let mut target_tests = package
        .targets
        .iter()
        .filter(|target| {
            target
                .kind
                .iter()
                .any(|kind| matches!(kind, cargo_metadata::TargetKind::Test))
        })
        .map(|target| {
            let normalized =
                normalize_workspace_path(workspace_root, target.src_path.as_std_path())?;
            Ok(display_relative(workspace_root, &normalized))
        })
        .collect::<Result<Vec<_>>>()?;
    target_tests.sort();
    let mut features = package.features.keys().cloned().collect::<Vec<_>>();
    features.sort();
    Ok(PackageSnapshot {
        name: package.name.to_string(),
        manifest_path,
        package_root,
        agent,
        direct_dependencies: sorted_lookup(direct, &package.name),
        reverse_dependencies: sorted_lookup(reverse, &package.name),
        target_names,
        target_tests,
        features,
    })
}

fn sorted_lookup(map: &HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
    let mut values = match map.get(key) {
        Some(values) => values.clone(),
        None => Vec::new(),
    };
    values.sort();
    values.dedup();
    values
}

fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize path {}", path.display()))
}

fn normalize_manifest_path(path: &Path) -> Result<PathBuf> {
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

fn normalize_workspace_path(root: &Path, path: &Path) -> Result<PathBuf> {
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

fn display_relative(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root).ok() {
        Some(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ if path == root => ".".to_string(),
        _ => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock moved backwards")
            .as_nanos();
        workspace_root()
            .expect("workspace root")
            .join("target")
            .join("cargo-vrc-tests")
            .join(format!("{prefix}-{stamp}"))
    }

    #[test]
    fn normalize_workspace_path_accepts_paths_inside_root() {
        let root = unique_path("workspace-root");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested directory");
        let file = nested.join("Cargo.toml");
        fs::write(&file, "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n")
            .expect("write manifest");

        let root = fs::canonicalize(&root).expect("canonicalize root");
        let normalized = normalize_workspace_path(&root, &file).expect("normalize path");
        assert!(normalized.starts_with(&root));

        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn normalize_manifest_path_requires_cargo_toml() {
        let root = unique_path("workspace-manifest");
        fs::create_dir_all(&root).expect("create root directory");
        let manifest = root.join("not-a-manifest.txt");
        fs::write(&manifest, "manifest fixture").expect("write manifest-like file");

        let err = normalize_manifest_path(&manifest).expect_err("reject non-manifest path");
        assert!(err.to_string().contains("Cargo.toml"));

        let _ = fs::remove_file(&manifest);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn normalize_manifest_path_rejects_traversal_outside_workspace() {
        // Negative test for HLT-023-INPUT-BOUNDARY-GAP: confirm that a
        // user-supplied manifest path that canonicalizes outside the workspace
        // is refused before it can reach `MetadataCommand::exec`.
        //
        // The OS-provided ephemeral directory canonicalizes to a location
        // outside the compile-time workspace root on macOS (e.g.
        // /private/var/...), so a manifest written there is rejected by the
        // allowlist enforced by `normalize_manifest_path`.
        let outside_root = std::env::temp_dir().join(format!(
            "cargo-vrc-outside-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock moved backwards")
                .as_nanos()
        ));
        fs::create_dir_all(&outside_root).expect("create outside root");
        let outside_manifest = outside_root.join("Cargo.toml");
        fs::write(
            &outside_manifest,
            "[package]\nname = \"hostile\"\nversion = \"0.0.0\"\n",
        )
        .expect("write outside manifest");

        let err = normalize_manifest_path(&outside_manifest)
            .expect_err("reject manifest outside workspace");
        let message = err.to_string();
        assert!(
            message.contains("escapes workspace root"),
            "unexpected error: {message}"
        );

        let _ = fs::remove_file(&outside_manifest);
        let _ = fs::remove_dir_all(&outside_root);
    }

    #[test]
    fn normalize_workspace_path_rejects_paths_outside_root() {
        let root = unique_path("workspace-root");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested directory");
        let inside = nested.join("Cargo.toml");
        fs::write(&inside, "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n")
            .expect("write manifest");

        let outside = unique_path("workspace-outside.toml");
        fs::write(
            &outside,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .expect("write outside manifest");

        let root = fs::canonicalize(&root).expect("canonicalize root");
        let err = normalize_workspace_path(&root, &outside).expect_err("reject outside path");
        assert!(err.to_string().contains("escapes workspace root"));

        let _ = fs::remove_file(&inside);
        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&root);
    }
}
