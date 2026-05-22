use std::path::Path;

use anyhow::Result;
use cargo_vrc::{WorkspaceSnapshot, load_workspace};

use super::*;

#[path = "scan_support.rs"]
mod support;
pub(crate) use support::*;

pub fn scan_workspace(manifest_path: Option<&Path>) -> Result<ScanReport> {
    let snapshot = load_workspace(manifest_path)?;
    let mut findings = Vec::new();
    for package in &snapshot.packages {
        findings.extend(scan_package(&snapshot, package)?);
    }
    for finding in &mut findings {
        if finding.existing_exception.is_some() {
            finding.severity = "note".to_string();
        }
    }
    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.class_id.cmp(&right.class_id))
    });
    Ok(ScanReport {
        generated_at: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        workspace_root: display_workspace_root(),
        findings,
        repair_hint: report_repair_hint(),
    })
}

fn scan_package(
    snapshot: &WorkspaceSnapshot,
    package: &cargo_vrc::workspace::PackageSnapshot,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    let existing = |class_id: &str| existing_exception(&package.agent, class_id);

    if package.agent.purpose.trim().is_empty()
        || package.agent.local_validate.is_empty()
        || package.agent.invariants.is_empty()
    {
        findings.push(Finding {
            class_id: "missing-local-validation-metadata".to_string(),
            severity: "warning".to_string(),
            confidence: 0.98,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!(
                "{} is missing purpose, invariants, or local validation metadata",
                package.name
            ),
            suggested_fix: "Populate package.metadata.agent with purpose, invariants, and local_validate commands.".to_string(),
            existing_exception: existing("missing-local-validation-metadata"),
        });
    }

    if looks_like_junk_drawer(&package.name) {
        findings.push(Finding {
            class_id: "junk-drawer-shared-crate".to_string(),
            severity: "warning".to_string(),
            confidence: 0.85,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!("{} looks like a generic shared crate", package.name),
            suggested_fix:
                "Rename or split the crate by domain so ownership and blast radius stay clear."
                    .to_string(),
            existing_exception: existing("junk-drawer-shared-crate"),
        });
    }

    let workspace_has_semver = workspace_has_semver_profile(
        &snapshot.workspace_agent.ci_profiles,
        &snapshot.workspace_agent.shared_contracts,
    );

    let source_file_count = package_source_file_count(&package.package_root);

    findings.extend(scan_function_lengths(
        &snapshot.workspace_root,
        &package.manifest_path,
        package,
        existing,
    )?);
    findings.extend(scan_unsafe_blocks(
        &snapshot.workspace_root,
        &package.manifest_path,
        package,
        existing,
    )?);

    if source_file_count > 1
        && !workspace_has_semver
        && package.agent.local_validate.iter().any(|cmd| {
            command_has_semver_signal(cmd)
                || cmd.contains("cargo check")
                || cmd.contains("cargo test")
                || cmd.contains("cargo nextest")
        })
    {
        findings.push(Finding {
            class_id: "semver-profile-check-missing".to_string(),
            severity: "warning".to_string(),
            confidence: 0.8,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!(
                "{} has validation commands that look like a semver-safe profile but no workspace profile is configured",
                package.name
            ),
            suggested_fix: "Add a semver-safe cargo profile or simplify validation commands so they don't imply profile-based compatibility guarantees.".to_string(),
            existing_exception: existing("semver-profile-check-missing"),
        });
    }

    if package.agent.public_api
        && !package
            .agent
            .local_validate
            .iter()
            .any(|command| command.contains("--doc"))
    {
        findings.push(Finding {
            class_id: "public-api-no-doctest-coverage".to_string(),
            severity: "warning".to_string(),
            confidence: 0.92,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!(
                "{} is public_api=true but local validation omits doctests",
                package.name
            ),
            suggested_fix:
                "Add `cargo test -p <crate> --doc` to local_validate or document an AER."
                    .to_string(),
            existing_exception: existing("public-api-no-doctest-coverage"),
        });
    }

    if source_file_count >= 25
        && package.agent.local_validate.iter().any(|cmd| {
            cmd.contains("cargo run -p cargo-aer")
                || cmd.contains("cargo aer")
                || command_has_semver_signal(cmd)
        })
        && !package
            .agent
            .local_validate
            .iter()
            .any(|cmd| cmd.contains("--incompatible"))
    {
        findings.push(Finding {
            class_id: "workspace-validation-drift".to_string(),
            severity: "warning".to_string(),
            confidence: 0.7,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!(
                "{} looks like a wide workspace package but its validation commands do not advertise drift/semver handling",
                package.name
            ),
            suggested_fix: "Make the validation command explicitly acknowledge compatibility drift or split the package into smaller modules.".to_string(),
            existing_exception: existing("workspace-validation-drift"),
        });
    }

    let hidden_io_detected = package_has_hidden_io(&package.package_root);

    if looks_like_core_layer(&package.agent, &package.name) && hidden_io_detected {
        findings.push(Finding {
            class_id: "hidden-side-effects".to_string(),
            severity: "warning".to_string(),
            confidence: 0.73,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: "File appears to perform I/O or env access inside a core or pure layer"
                .to_string(),
            suggested_fix: "Move I/O and env access to an adapter boundary and inject the result into the core layer.".to_string(),
            existing_exception: existing("hidden-side-effects"),
        });
    }

    Ok(findings)
}
