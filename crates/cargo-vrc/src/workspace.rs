use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::{Metadata, MetadataCommand, Package};

use crate::model::{PackageAgentMetadata, WorkspaceAgentMetadata};

#[path = "workspace_paths.rs"]
mod paths;
use paths::{display_relative, normalize_manifest_path, normalize_workspace_path, workspace_root};

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
    let workspace_root = paths::normalize_existing_path(metadata.workspace_root.as_std_path())?;
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
    paths::sorted_lookup(map, key)
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
