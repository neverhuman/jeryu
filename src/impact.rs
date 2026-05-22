//! Owner: Change Impact Analysis
//! Proof: `cargo test -p jeryu -- impact`
//! Invariants: Conservative widening on empty diff or broad config change; docs-only skips heavy validation

use anyhow::Result;

use crate::decision::{ImpactDecision, ImpactLane};
use crate::gitlab_client::GitlabClient;

#[path = "impact_repo.rs"]
mod impact_repo;

pub async fn plan_for_push(
    client: &GitlabClient,
    project_id: i64,
    before: &str,
    after: &str,
) -> Result<ImpactDecision> {
    if before.chars().all(|c| c == '0') {
        return Ok(ImpactDecision {
            project_id,
            before: before.to_string(),
            after: after.to_string(),
            affected_paths: Vec::new(),
            selected_lanes: vec![ImpactLane::Full],
            reason_codes: vec!["new_branch_push".to_string(), "widened_to_full".to_string()],
            widened_to_full: true,
        });
    }

    let changed_paths =
        impact_repo::changed_paths_from_repo(client, project_id, before, after).await?;
    Ok(plan_from_changed_paths(
        project_id,
        before,
        after,
        changed_paths,
    ))
}

pub fn plan_from_changed_paths(
    project_id: i64,
    before: &str,
    after: &str,
    changed_paths: Vec<String>,
) -> ImpactDecision {
    let mut lanes = Vec::new();
    let mut reasons = Vec::new();
    let mut widened_to_full = false;

    if changed_paths.is_empty() {
        lanes.push(ImpactLane::Full);
        reasons.push("empty_diff".to_string());
        reasons.push("widened_to_full".to_string());
        widened_to_full = true;
    } else {
        let touches_src = changed_paths
            .iter()
            .any(|p| p.starts_with("src/") && p.ends_with(".rs"));
        let touches_tests = changed_paths.iter().any(|p| p.starts_with("tests/"));
        let touches_broad = changed_paths.iter().any(|p| {
            p.starts_with(".github/")
                || p.contains(".gitlab-ci")
                || p == "Cargo.toml"
                || p == "Cargo.lock"
                || p == "rust-toolchain.toml"
                || p.starts_with(".cargo/")
        });
        let touches_docs_only = !touches_src
            && !touches_tests
            && !touches_broad
            && changed_paths.iter().all(|p| {
                p.ends_with(".md") || p.starts_with("docs/") || p == "LICENSE" || p == ".gitignore"
            });

        if touches_broad {
            lanes.push(ImpactLane::Full);
            reasons.push("broad_config_change".to_string());
            widened_to_full = true;
        } else if touches_docs_only {
            lanes.push(ImpactLane::DocsOnly);
            reasons.push("docs_only_change".to_string());
        } else {
            if touches_src {
                lanes.push(ImpactLane::Unit);
                reasons.push("changed_src_rust".to_string());
            }
            if touches_tests {
                lanes.push(ImpactLane::Integration);
                reasons.push("changed_tests".to_string());
            }
            if lanes.is_empty() {
                lanes.push(ImpactLane::Full);
                reasons.push("unknown_file_change".to_string());
                reasons.push("widened_to_full".to_string());
                widened_to_full = true;
            }
        }
    }

    lanes.sort_by_key(|lane| match lane {
        ImpactLane::Full => 0,
        ImpactLane::Unit => 1,
        ImpactLane::Integration => 2,
        ImpactLane::DocsOnly => 3,
    });
    lanes.dedup();

    ImpactDecision {
        project_id,
        before: before.to_string(),
        after: after.to_string(),
        affected_paths: changed_paths,
        selected_lanes: lanes,
        reason_codes: reasons,
        widened_to_full,
    }
}

fn build_dynamic_yaml(plan: &ImpactDecision) -> String {
    if plan.selected_lanes.contains(&ImpactLane::Full) {
        return "image: rust:latest\n\nfull-validation:\n  script:\n    - cargo test --all-targets --all-features\n".to_string();
    }

    let mut jobs = String::from("image: rust:latest\n\n");
    if plan.selected_lanes.contains(&ImpactLane::Unit) {
        jobs.push_str("unit-validation:\n  script:\n    - cargo test --lib --bins\n\n");
    }
    if plan.selected_lanes.contains(&ImpactLane::Integration) {
        jobs.push_str("integration-validation:\n  script:\n    - cargo test --tests\n\n");
    }
    if plan.selected_lanes.contains(&ImpactLane::DocsOnly) {
        jobs.push_str("docs-validation:\n  script:\n    - echo 'Docs-only change detected; no heavy validation required.'\n");
    }
    jobs
}

pub fn render_plan_payload(plan: &ImpactDecision) -> serde_json::Value {
    serde_json::json!({
        "project_id": plan.project_id,
        "before": plan.before,
        "after": plan.after,
        "affected_paths": plan.affected_paths,
        "selected_lanes": plan.selected_lanes,
        "reason_codes": plan.reason_codes,
        "widened_to_full": plan.widened_to_full,
        "generated_ci_yaml": build_dynamic_yaml(plan),
    })
}

#[cfg(test)]
#[path = "impact_tests.rs"]
mod tests;
