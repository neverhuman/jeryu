use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};

use crate::{PackageSnapshot, WorkspaceSnapshot};

pub(crate) fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }
    builder
        .build()
        .context("failed to compile owned_paths globset")
}

pub(crate) fn boundary_trigger(path: &str, package: &PackageSnapshot) -> bool {
    let path = Path::new(path);
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    file_name == "Cargo.toml"
        || (package.agent.public_api && (file_name == "lib.rs" || file_name == "mod.rs"))
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

pub(crate) fn public_surfaces(package: &PackageSnapshot) -> Vec<String> {
    if !package.agent.entrypoints.is_empty() {
        return package.agent.entrypoints.clone();
    }
    package.target_names.clone()
}

pub(crate) fn risk_tags(package: &PackageSnapshot) -> Vec<String> {
    let mut tags = vec![if package.agent.risk.is_empty() {
        "risk:unspecified".to_string()
    } else {
        format!("risk:{}", package.agent.risk)
    }];
    if package.agent.public_api {
        tags.push("public-api".to_string());
    }
    if !package.agent.exceptions.is_empty() {
        tags.push("has-aer".to_string());
    }
    tags
}

pub(crate) fn instruction_locations(
    workspace_root: &Path,
    package: &PackageSnapshot,
) -> Vec<String> {
    let mut locations = Vec::new();
    for path in [
        workspace_root.join("AGENTS.md"),
        workspace_root.join("CLAUDE.md"),
        workspace_root.join(".github/copilot-instructions.md"),
        package.package_root.join("AGENTS.md"),
    ] {
        if path.exists() {
            locations.push(display_relative(workspace_root, &path));
        }
    }
    locations
}

pub(crate) fn context_roots(workspace_root: &Path, package: &PackageSnapshot) -> Vec<String> {
    let mut roots = vec![display_relative(workspace_root, &package.package_root)];
    for suffix in ["src", "tests", "examples"] {
        let candidate = package.package_root.join(suffix);
        if candidate.exists() {
            roots.push(display_relative(workspace_root, &candidate));
        }
    }
    for location in instruction_locations(workspace_root, package) {
        roots.push(location);
    }
    roots.sort();
    roots.dedup();
    roots
}

pub(crate) fn owned_path_display(package: &PackageSnapshot) -> Vec<String> {
    if package.agent.owned_paths.is_empty() {
        return vec!["<package-root>".to_string()];
    }
    package.agent.owned_paths.clone()
}

pub(crate) fn proof_density(package: &PackageSnapshot) -> f64 {
    let proof_points = package.agent.invariants.len()
        + package.agent.local_validate.len()
        + package.agent.boundary_validate.len()
        + package.agent.exceptions.len();
    let entrypoints = package.agent.entrypoints.len().max(1);
    let density = proof_points as f64 / entrypoints as f64;
    (density * 100.0).round() / 100.0
}

pub(crate) fn api_surface_hash(package: &PackageSnapshot) -> String {
    let mut hasher = Sha256::new();
    hasher.update(package.name.as_bytes());
    hasher.update(if package.agent.public_api {
        &b"public"[..]
    } else {
        &b"private"[..]
    });
    for item in public_surfaces(package) {
        hasher.update(item.as_bytes());
    }
    for feature in &package.features {
        hasher.update(feature.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn collect_profile_commands(
    snapshot: &WorkspaceSnapshot,
    profile_name: &str,
    needle: &str,
) -> Vec<String> {
    match snapshot
        .workspace_agent
        .ci_profiles
        .iter()
        .find(|profile| profile.name == profile_name)
    {
        Some(profile) => profile
            .commands
            .iter()
            .filter(|command| command.contains(needle))
            .cloned()
            .collect::<Vec<_>>(),
        None => Vec::new(),
    }
}

pub(crate) fn estimated_cost(package: &PackageSnapshot) -> String {
    let local = package.agent.local_validate.len();
    let boundary = package.agent.boundary_validate.len();
    let harnesses = package.target_tests.len();
    match local + boundary + harnesses {
        0..=2 => "low".to_string(),
        3..=5 => "medium".to_string(),
        _ => "high".to_string(),
    }
}

pub(crate) fn required_for_change_types(package: &PackageSnapshot) -> Vec<String> {
    let mut change_types = vec!["leaf-bugfix".to_string(), "invariant-change".to_string()];
    if package.agent.public_api {
        change_types.push("public-api-change".to_string());
    }
    if !package.features.is_empty() {
        change_types.push("feature-change".to_string());
    }
    change_types.push("manifest-change".to_string());
    change_types
}

pub(crate) fn display_relative(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ if path == root => ".".to_string(),
        _ => path.display().to_string(),
    }
}

pub(crate) fn display_workspace_root() -> String {
    ".".to_string()
}

pub(crate) fn generated_at() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}
