use std::path::Path;

use anyhow::{Context, Result};

use crate::workspace::WorkspaceSnapshot;

use super::{build_vrc_plan, planner_support::verify_workspace_fields};

pub fn explain_subject(snapshot: &WorkspaceSnapshot, subject: &str) -> Result<serde_json::Value> {
    let subject_path = Path::new(subject);
    if subject_path.exists() || subject.contains('/') || subject.contains('\\') {
        let plan = build_vrc_plan(snapshot, &[subject_path.to_path_buf()])?;
        return serde_json::to_value(plan).context("failed to serialize explanation plan");
    }

    let matches = snapshot
        .packages
        .iter()
        .filter(|package| {
            package.name == subject
                || package
                    .agent
                    .entrypoints
                    .iter()
                    .any(|entry| entry == subject)
        })
        .map(|package| {
            serde_json::json!({
                "arc": package.name,
                "purpose": package.agent.purpose,
                "entrypoints": package.agent.entrypoints,
                "invariants": package.agent.invariants,
                "local_validate": package.agent.local_validate,
                "boundary_validate": package.agent.boundary_validate,
                "reverse_dependencies": package.reverse_dependencies,
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "subject": subject,
        "matches": matches,
    }))
}

pub fn verify_workspace(snapshot: &WorkspaceSnapshot) -> crate::model::VerificationReport {
    verify_workspace_fields(snapshot)
}
