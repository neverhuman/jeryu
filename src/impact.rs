//! Owner: Change Impact Analysis
//! Proof: `cargo test -p jeryu -- impact`
//! Invariants: Conservative widening on empty diff or broad config change; docs-only skips heavy validation

use anyhow::{Context, Result};
use git2::{Cred, DiffOptions, FetchOptions, Oid, RemoteCallbacks, Repository, build::RepoBuilder};

use crate::decision::{ImpactDecision, ImpactLane};
use crate::gitlab_client::GitlabClient;

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

    let changed_paths = changed_paths_from_repo(client, project_id, before, after).await?;
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

async fn changed_paths_from_repo(
    client: &GitlabClient,
    project_id: i64,
    before: &str,
    after: &str,
) -> Result<Vec<String>> {
    let project = client.get_project(project_id).await?;
    let clone_dir = std::env::temp_dir().join(format!("jeryu-impact-{}", uuid::Uuid::new_v4()));
    let repo = clone_project(client, &project.web_url, &clone_dir)
        .with_context(|| format!("cloning project {} for impact analysis", project.web_url))?;

    let before_commit = repo.find_commit(Oid::from_str(before)?)?;
    let after_commit = repo.find_commit(Oid::from_str(after)?)?;
    let before_tree = before_commit.tree()?;
    let after_tree = after_commit.tree()?;
    let mut diff_opts = DiffOptions::new();
    let diff =
        repo.diff_tree_to_tree(Some(&before_tree), Some(&after_tree), Some(&mut diff_opts))?;

    let mut changed_paths = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(path) = delta.new_file().path().or(delta.old_file().path()) {
                changed_paths.push(path.to_string_lossy().to_string());
            }
            true
        },
        None,
        None,
        None,
    )?;

    let _ = std::fs::remove_dir_all(&clone_dir);
    Ok(changed_paths)
}

fn clone_project(
    client: &GitlabClient,
    web_url: &str,
    clone_dir: &std::path::Path,
) -> Result<Repository> {
    let mut callbacks = RemoteCallbacks::new();
    let pat = match client.pat_value_for_clone() {
        Some(p) => p,
        None => String::new(),
    };
    callbacks.credentials(move |_url, _username, _types| Cred::userpass_plaintext("oauth2", &pat));

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    let mut builder = RepoBuilder::new();
    builder.fetch_options(fetch_options);
    builder
        .clone(&format!("{}.git", web_url), clone_dir)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_changes_select_unit_lane() {
        let plan = plan_from_changed_paths(1, "a", "b", vec!["src/main.rs".to_string()]);
        assert!(plan.selected_lanes.contains(&ImpactLane::Unit));
        assert!(!plan.selected_lanes.contains(&ImpactLane::Full));
    }

    #[test]
    fn config_changes_widen_to_full() {
        let plan = plan_from_changed_paths(1, "a", "b", vec!["Cargo.toml".to_string()]);
        assert_eq!(plan.selected_lanes, vec![ImpactLane::Full]);
        assert!(plan.widened_to_full);
    }

    #[test]
    fn markdown_only_selects_docs_lane() {
        let plan = plan_from_changed_paths(1, "a", "b", vec!["README.md".to_string()]);
        assert_eq!(plan.selected_lanes, vec![ImpactLane::DocsOnly]);
        assert!(!plan.widened_to_full);
        assert!(plan.reason_codes.contains(&"docs_only_change".to_string()));
    }

    #[test]
    fn multiple_markdown_files_select_docs_lane() {
        let plan = plan_from_changed_paths(
            1,
            "a",
            "b",
            vec![
                "README.md".to_string(),
                "docs/architecture.md".to_string(),
                "API.md".to_string(),
            ],
        );
        assert_eq!(plan.selected_lanes, vec![ImpactLane::DocsOnly]);
        assert!(!plan.widened_to_full);
    }

    #[test]
    fn markdown_plus_src_selects_unit_not_full() {
        let plan = plan_from_changed_paths(
            1,
            "a",
            "b",
            vec!["README.md".to_string(), "src/pool.rs".to_string()],
        );
        assert!(plan.selected_lanes.contains(&ImpactLane::Unit));
        assert!(!plan.selected_lanes.contains(&ImpactLane::Full));
    }

    #[test]
    fn rust_toolchain_triggers_full() {
        let plan = plan_from_changed_paths(1, "a", "b", vec!["rust-toolchain.toml".to_string()]);
        assert_eq!(plan.selected_lanes, vec![ImpactLane::Full]);
        assert!(plan.widened_to_full);
    }

    #[test]
    fn cargo_dir_triggers_full() {
        let plan = plan_from_changed_paths(1, "a", "b", vec![".cargo/config.toml".to_string()]);
        assert_eq!(plan.selected_lanes, vec![ImpactLane::Full]);
        assert!(plan.widened_to_full);
    }

    #[test]
    fn test_only_changes_select_integration() {
        let plan = plan_from_changed_paths(1, "a", "b", vec!["tests/pool_tests.rs".to_string()]);
        assert!(plan.selected_lanes.contains(&ImpactLane::Integration));
        assert!(!plan.selected_lanes.contains(&ImpactLane::Full));
    }

    #[test]
    fn gitignore_selects_docs_not_full() {
        let plan = plan_from_changed_paths(1, "a", "b", vec![".gitignore".to_string()]);
        assert_eq!(plan.selected_lanes, vec![ImpactLane::DocsOnly]);
    }

    #[test]
    fn unknown_file_type_selects_non_code() {
        let plan = plan_from_changed_paths(1, "a", "b", vec!["data/fixture.json".to_string()]);
        assert_eq!(plan.selected_lanes, vec![ImpactLane::Full]);
        assert!(plan.widened_to_full);
        assert!(
            plan.reason_codes
                .contains(&"unknown_file_change".to_string())
        );
    }
}
