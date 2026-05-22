use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::model::VerificationReport;
use crate::{PackageSnapshot, WorkspaceSnapshot};

#[path = "planner_support_metrics.rs"]
mod metrics;
#[path = "planner_support_paths.rs"]
mod paths;
pub use metrics::context_metrics;
pub(crate) use paths::{
    api_surface_hash, boundary_trigger, build_globset, collect_profile_commands, context_roots,
    display_relative, display_workspace_root, estimated_cost, generated_at, instruction_locations,
    owned_path_display, proof_density, public_surfaces, required_for_change_types, risk_tags,
};

pub(crate) fn verify_workspace_fields(snapshot: &WorkspaceSnapshot) -> VerificationReport {
    let mut report = VerificationReport::default();

    if snapshot.workspace_agent.validation_order.is_empty() {
        report
            .warnings
            .push("workspace.metadata.agent.validation_order is empty".to_string());
    }
    if snapshot.workspace_agent.instruction_roots.is_empty() {
        report
            .warnings
            .push("workspace.metadata.agent.instruction_roots is empty".to_string());
    }

    for package in &snapshot.packages {
        if package.agent.purpose.trim().is_empty() {
            report.errors.push(format!(
                "{} is missing package.metadata.agent.purpose",
                package.name
            ));
        }
        if package.agent.invariants.is_empty() {
            report
                .warnings
                .push(format!("{} is missing explicit invariants", package.name));
        }
        if package.agent.local_validate.is_empty() {
            report.errors.push(format!(
                "{} is missing package.metadata.agent.local_validate",
                package.name
            ));
        }
        if package.agent.owned_paths.is_empty() {
            report.warnings.push(format!(
                "{} is missing package.metadata.agent.owned_paths; path matching will fall back to package roots",
                package.name
            ));
        }
        if package.agent.public_api && package.agent.boundary_validate.is_empty() {
            report.errors.push(format!(
                "{} is marked public_api=true but has no boundary_validate commands",
                package.name
            ));
        }
        let local_agents = package.package_root.join("AGENTS.md");
        if !local_agents.exists() {
            report.warnings.push(format!(
                "{} has no local AGENTS.md; consider adding crate-specific guidance",
                package.name
            ));
        }
    }

    report
}

pub(crate) fn normalize_changed_paths(
    workspace_root: &Path,
    changed_paths: &[PathBuf],
) -> Vec<String> {
    let mut normalized = changed_paths
        .iter()
        .map(|path| {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                workspace_root.join(path)
            };
            display_relative(workspace_root, &absolute)
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

pub(crate) fn matched_paths(
    snapshot: &WorkspaceSnapshot,
    package: &PackageSnapshot,
    changed_paths: &[String],
) -> Vec<String> {
    let matcher = build_globset(&package.agent.owned_paths).ok();
    let package_root = display_relative(&snapshot.workspace_root, &package.package_root);
    let mut hits = BTreeSet::new();
    for changed in changed_paths {
        if changed == &package_root || changed.starts_with(&(package_root.clone() + "/")) {
            hits.insert(changed.clone());
            continue;
        }
        if let Some(matcher) = &matcher {
            let changed_path = Path::new(changed);
            if package_root == "." && matcher.is_match(changed_path) {
                hits.insert(changed.clone());
                continue;
            }
            if let Ok(stripped) = changed_path.strip_prefix(&package_root)
                && matcher.is_match(stripped)
            {
                hits.insert(changed.clone());
            }
        }
    }
    hits.into_iter().collect()
}
