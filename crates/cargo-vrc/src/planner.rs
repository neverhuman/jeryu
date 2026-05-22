use std::path::PathBuf;

use anyhow::Result;

use crate::model::{
    AgentMap, AgentMember, SelectedArc, SelectedTest, TestEntry, TestMap, ValidationCommands,
    VrcPlan,
};
use crate::workspace::WorkspaceSnapshot;

#[path = "planner_support.rs"]
mod planner_support;

use self::planner_support::{
    api_surface_hash, boundary_trigger, collect_profile_commands, context_roots, display_relative,
    display_workspace_root, estimated_cost, generated_at, instruction_locations, matched_paths,
    normalize_changed_paths, owned_path_display, proof_density, public_surfaces,
    required_for_change_types, risk_tags,
};
pub use planner_support::context_metrics;
pub fn build_agent_map(snapshot: &WorkspaceSnapshot) -> AgentMap {
    let members = snapshot
        .packages
        .iter()
        .map(|package| AgentMember {
            name: package.name.clone(),
            manifest_path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            package_root: display_relative(&snapshot.workspace_root, &package.package_root),
            direct_dependencies: package.direct_dependencies.clone(),
            reverse_dependencies: package.reverse_dependencies.clone(),
            public_surfaces: public_surfaces(package),
            risk_tags: risk_tags(package),
            instruction_locations: instruction_locations(&snapshot.workspace_root, package),
            validation_commands: ValidationCommands {
                local: package.agent.local_validate.clone(),
                boundary: package.agent.boundary_validate.clone(),
            },
            api_surface_hash: api_surface_hash(package),
            proof_density: proof_density(package),
            context_roots: context_roots(&snapshot.workspace_root, package),
            exception_refs: package.agent.exceptions.clone(),
        })
        .collect();

    AgentMap {
        generated_at: generated_at(),
        workspace_root: display_workspace_root(),
        validation_order: snapshot.workspace_agent.validation_order.clone(),
        shared_contracts: snapshot.workspace_agent.shared_contracts.clone(),
        ci_profiles: snapshot.workspace_agent.ci_profiles.clone(),
        instruction_roots: snapshot.workspace_agent.instruction_roots.clone(),
        members,
    }
}

pub fn build_test_map(snapshot: &WorkspaceSnapshot) -> TestMap {
    let smoke_tests = collect_profile_commands(snapshot, "pull-request", "smoke");
    let e2e_gates = collect_profile_commands(snapshot, "scheduled-hardening", "e2e");
    let entries = snapshot
        .packages
        .iter()
        .map(|package| TestEntry {
            arc: package.name.clone(),
            source_roots: owned_path_display(package),
            unit_tests: package
                .agent
                .local_validate
                .iter()
                .filter(|command| !command.contains("--doc"))
                .cloned()
                .collect(),
            doctests: package
                .agent
                .local_validate
                .iter()
                .filter(|command| command.contains("--doc"))
                .cloned()
                .collect(),
            integration_harnesses: package.target_tests.clone(),
            reverse_dependency_tests: package.agent.boundary_validate.clone(),
            smoke_tests: smoke_tests.clone(),
            e2e_gates: e2e_gates.clone(),
            selection_reason: if package.agent.public_api {
                "public surface changes require reverse dependency and contract awareness"
                    .to_string()
            } else {
                "leaf ARC changes usually stop at local proof unless a manifest or boundary moves"
                    .to_string()
            },
            estimated_cost: estimated_cost(package),
            required_for_change_types: required_for_change_types(package),
        })
        .collect();

    TestMap {
        generated_at: generated_at(),
        workspace_root: display_workspace_root(),
        entries,
    }
}

pub fn build_vrc_plan(snapshot: &WorkspaceSnapshot, changed_paths: &[PathBuf]) -> Result<VrcPlan> {
    let normalized_paths = normalize_changed_paths(&snapshot.workspace_root, changed_paths);
    let mut selected_arcs = Vec::new();
    let mut selected_tests = Vec::new();
    let mut rationale = Vec::new();
    let mut boundary_required = false;

    for package in &snapshot.packages {
        let hits = matched_paths(snapshot, package, &normalized_paths);
        if hits.is_empty() {
            continue;
        }
        let requires_boundary = hits.iter().any(|path| boundary_trigger(path, package));
        boundary_required |= requires_boundary;
        let reason = if requires_boundary {
            format!(
                "Matched {} and crossed a public or manifest boundary",
                hits.join(", ")
            )
        } else {
            format!("Matched owned paths {}", hits.join(", "))
        };
        rationale.push(format!("Selected {} because {}", package.name, reason));
        selected_arcs.push(SelectedArc {
            name: package.name.clone(),
            reason: reason.clone(),
            local_validate: package.agent.local_validate.clone(),
            boundary_validate: if requires_boundary {
                package.agent.boundary_validate.clone()
            } else {
                Vec::new()
            },
            public_api: package.agent.public_api,
        });

        for command in &package.agent.local_validate {
            selected_tests.push(SelectedTest {
                arc: package.name.clone(),
                command: command.clone(),
                ring: if command.contains("--doc") {
                    "doctest".to_string()
                } else {
                    "local".to_string()
                },
                selection_reason: "local proof is always required for matched ARCs".to_string(),
            });
        }
        if requires_boundary {
            for command in &package.agent.boundary_validate {
                selected_tests.push(SelectedTest {
                    arc: package.name.clone(),
                    command: command.clone(),
                    ring: "boundary".to_string(),
                    selection_reason:
                        "public API or manifest changes widened the validation radius".to_string(),
                });
            }
        }
    }

    let stop_condition = if selected_arcs.is_empty() {
        rationale.push("No ARC matched the changed paths; this is likely a documentation or root-policy change.".to_string());
        "no-arc-match".to_string()
    } else if boundary_required {
        "stop after mapped reverse dependency and contract rings".to_string()
    } else {
        "stop after local compile, tests, and doctests".to_string()
    };

    let skipped_rings = if boundary_required {
        vec!["full-e2e".to_string()]
    } else {
        vec![
            "reverse-dependency".to_string(),
            "contract".to_string(),
            "smoke".to_string(),
            "full-e2e".to_string(),
        ]
    };

    Ok(VrcPlan {
        generated_at: generated_at(),
        changed_paths: normalized_paths,
        selected_arcs,
        selected_tests,
        stop_condition,
        skipped_rings,
        rationale,
    })
}

#[path = "planner_subject.rs"]
mod subject;

pub use subject::{explain_subject, verify_workspace};

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;
