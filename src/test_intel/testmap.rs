//! Owner: VTI Test Intelligence subsystem — testmap.toml parser
//! Proof: `cargo nextest run -p jeryu -- test_intel::testmap`
//! Invariants: Parsed test maps preserve lane semantics and reject ambiguous ownership where possible.
//! Parses `.jeryu/testmap.toml` files for external workspace integration.
//!
//! This module provides VTI support for repos *other than* JeRyu itself
//! (e.g., the dougx workspace) by reading a TOML-based subsystem map.

use super::subsystem::glob_match;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

// ---------------------------------------------------------------------------
// TOML schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TestMap {
    pub policy: TestMapPolicy,
    pub global_invalidators: TestMapPaths,
    #[serde(default)]
    pub docs: Option<TestMapPaths>,
    #[serde(default)]
    pub subsystem: Vec<TestMapSubsystem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestMapPolicy {
    pub full_on_unknown: bool,
    pub min_confidence: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestMapPaths {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestMapSubsystem {
    pub id: String,
    #[serde(default)]
    pub description: String,
    pub paths: Vec<String>,
    pub ci_jobs: Vec<String>,
    #[serde(default)]
    pub cross_cutting: bool,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Load and parse a `.jeryu/testmap.toml` file.
pub fn load_testmap(path: &Path) -> Result<TestMap, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    toml::from_str(&content).map_err(|e| format!("failed to parse {}: {}", path.display(), e))
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// A plan produced by VTI for an external workspace.
#[derive(Debug, Clone)]
pub struct ExternalTestPlan {
    pub mode: ExternalPlanMode,
    pub selected_jobs: Vec<String>,
    pub skipped_jobs: Vec<String>,
    pub affected_subsystems: Vec<String>,
    pub confidence: f64,
    pub rationale: Vec<String>,
    pub changed_paths: Vec<String>,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternalPlanMode {
    Full,
    Selected,
    DocsOnly,
}

impl ExternalTestPlan {
    pub fn recovery_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }
}

/// Compute a test plan from a `.jeryu/testmap.toml` and a list of changed paths.
pub fn plan_from_testmap(map: &TestMap, changed_paths: &[String]) -> ExternalTestPlan {
    let mut rationale = Vec::new();

    // 1. Empty diff → full
    if changed_paths.is_empty() {
        return ExternalTestPlan {
            mode: ExternalPlanMode::Full,
            selected_jobs: vec![],
            skipped_jobs: vec![],
            affected_subsystems: vec![],
            confidence: 1.0,
            rationale: vec!["empty diff, running full pipeline".into()],
            changed_paths: vec![],
            fallback_reason: Some("empty diff".into()),
        };
    }

    // 2. Check global invalidators
    for path in changed_paths {
        for pattern in &map.global_invalidators.paths {
            if glob_match(pattern, path) {
                rationale.push(format!(
                    "global invalidator hit: '{}' matched pattern '{}'",
                    path, pattern
                ));
                return ExternalTestPlan {
                    mode: ExternalPlanMode::Full,
                    selected_jobs: vec![],
                    skipped_jobs: vec![],
                    affected_subsystems: vec![],
                    confidence: 1.0,
                    rationale,
                    changed_paths: changed_paths.to_vec(),
                    fallback_reason: Some(format!("global invalidator: {}", path)),
                };
            }
        }
    }

    // 3. Check docs-only
    let docs_patterns = map.docs.as_ref().map(|d| d.paths.as_slice()).unwrap_or(&[]);
    let all_docs = !changed_paths.is_empty()
        && changed_paths
            .iter()
            .all(|p| docs_patterns.iter().any(|pat| glob_match(pat, p)));
    if all_docs {
        rationale.push("all changed files match docs-only patterns".into());
        return ExternalTestPlan {
            mode: ExternalPlanMode::DocsOnly,
            selected_jobs: vec![],
            skipped_jobs: vec![],
            affected_subsystems: vec![],
            confidence: 1.0,
            rationale,
            changed_paths: changed_paths.to_vec(),
            fallback_reason: None,
        };
    }

    // 4. Match subsystems
    let mut affected: Vec<&TestMapSubsystem> = Vec::new();
    let mut unmatched_paths: Vec<&str> = Vec::new();

    for path in changed_paths {
        // Skip docs files when mixed with code
        if docs_patterns.iter().any(|pat| glob_match(pat, path)) {
            continue;
        }

        let mut matched = false;
        for sub in &map.subsystem {
            if sub.paths.iter().any(|pat| glob_match(pat, path)) {
                if !affected.iter().any(|a| a.id == sub.id) {
                    affected.push(sub);
                    rationale.push(format!(
                        "selected '{}' because '{}' matches its paths",
                        sub.id, path
                    ));
                }
                matched = true;
            }
        }
        if !matched {
            unmatched_paths.push(path);
        }
    }

    // 5. Unmatched paths → conservative recovery (if policy says so)
    if !unmatched_paths.is_empty() && map.policy.full_on_unknown {
        rationale.push(format!(
            "unmatched paths with full_on_unknown=true: {:?}",
            unmatched_paths
        ));
        return ExternalTestPlan {
            mode: ExternalPlanMode::Full,
            selected_jobs: vec![],
            skipped_jobs: vec![],
            affected_subsystems: affected.iter().map(|s| s.id.clone()).collect(),
            confidence: 0.0,
            rationale,
            changed_paths: changed_paths.to_vec(),
            fallback_reason: Some("unmatched paths".into()),
        };
    }

    // 6. Collect jobs from affected subsystems
    let mut selected_jobs: Vec<String> = Vec::new();
    let mut affected_ids: Vec<String> = Vec::new();
    let mut has_cross_cutting = false;

    for sub in &affected {
        affected_ids.push(sub.id.clone());
        if sub.cross_cutting {
            has_cross_cutting = true;
        }
        for job in &sub.ci_jobs {
            if !selected_jobs.contains(job) {
                selected_jobs.push(job.clone());
            }
        }
    }

    // 7. Compute all known jobs, then skipped = all - selected
    let all_jobs: Vec<String> = map
        .subsystem
        .iter()
        .flat_map(|s| s.ci_jobs.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let skipped_jobs: Vec<String> = all_jobs
        .iter()
        .filter(|j| !selected_jobs.contains(j))
        .cloned()
        .collect();

    // 8. Confidence scoring
    let mut confidence = 1.0_f64;
    if has_cross_cutting {
        confidence -= 0.10;
    }
    if affected.len() > 3 {
        confidence -= 0.05 * (affected.len() as f64 - 3.0);
    }
    if !unmatched_paths.is_empty() {
        confidence -= 0.15 * (unmatched_paths.len() as f64 / changed_paths.len() as f64);
    }
    confidence = confidence.clamp(0.0, 1.0);

    // 9. Check minimum confidence
    if confidence < map.policy.min_confidence {
        rationale.push(format!(
            "confidence {:.2} below threshold {:.2}; escalating to full",
            confidence, map.policy.min_confidence
        ));
        return ExternalTestPlan {
            mode: ExternalPlanMode::Full,
            selected_jobs: vec![],
            skipped_jobs: vec![],
            affected_subsystems: affected_ids,
            confidence,
            rationale,
            changed_paths: changed_paths.to_vec(),
            fallback_reason: Some(format!("low confidence: {:.2}", confidence)),
        };
    }

    ExternalTestPlan {
        mode: ExternalPlanMode::Selected,
        selected_jobs,
        skipped_jobs,
        affected_subsystems: affected_ids,
        confidence,
        rationale,
        changed_paths: changed_paths.to_vec(),
        fallback_reason: None,
    }
}

// ---------------------------------------------------------------------------
// GitLab child YAML generation for external workspace
// ---------------------------------------------------------------------------

const DOUX_MAIN_JOBS: &[&str] = &[
    "lint-cargo-fmt",
    "lint-cargo-clippy",
    "lint-shell",
    "lint-ci-schema",
    "vrc-plan",
    "compile-workspace",
    "build-release-artifacts",
    "build-bootstrap-musl",
    "build-enclave-server",
    "test-rust-cargo-vrc",
    "test-rust-veox-testctl",
    "test-rust-veox-deploy",
    "test-rust-nextest-1",
    "test-rust-nextest-2",
    "test-rust-nextest-3",
    "test-rust-nextest-4",
    "test-rust-contracts-core",
    "test-rust-contracts-persistence",
    "test-rust-contracts-warp",
    "test-rust-contracts-nht",
    "test-rust-contracts-retirement",
    "test-rust-nht-crypto",
    "test-frontend-warp",
    "test-frontend-nht",
    "test-public-deps",
    "test-security-hardening",
    "test-governance-retirement",
    "test-shell-container-parity",
    "test-security-ip-leak",
    "test-security-ip-guard",
    "test-security-ip-exfiltration",
    "test-smoke-warp-unified",
    "test-smoke-nht-datasets",
    "test-live-public-surface",
    "test-local-built",
    "publish-rc-dry-run",
    "test-local-rc",
    "audit-aer-scan",
    "audit-remaining-structural-findings",
    "audit-final-mile",
];

fn job_dependencies(job: &str) -> &'static [&'static str] {
    match job {
        "test-rust-nextest-1"
        | "test-rust-nextest-2"
        | "test-rust-nextest-3"
        | "test-rust-nextest-4" => &["compile-workspace"],
        "vrc-plan" => &["plan-tests"],
        "build-enclave-server" => &["build-release-artifacts", "build-bootstrap-musl"],
        "test-live-public-surface" | "test-local-built" | "publish-rc-dry-run" => {
            &["build-enclave-server"]
        }
        "test-local-rc" => &["publish-rc-dry-run"],
        _ => &[],
    }
}

fn add_job_with_dependencies(job: &str, selected: &mut BTreeSet<String>) {
    for dependency in job_dependencies(job) {
        if *dependency != "plan-tests" {
            add_job_with_dependencies(dependency, selected);
        }
    }
    selected.insert(job.to_string());
}

fn materialized_jobs(plan: &ExternalTestPlan) -> Vec<String> {
    let mut selected = BTreeSet::new();
    match plan.mode {
        ExternalPlanMode::Full => {
            for job in DOUX_MAIN_JOBS {
                add_job_with_dependencies(job, &mut selected);
            }
        }
        ExternalPlanMode::Selected => {
            for job in &plan.selected_jobs {
                add_job_with_dependencies(job, &mut selected);
            }
        }
        ExternalPlanMode::DocsOnly => {}
    }
    selected.into_iter().collect()
}

fn extract_top_level_yaml_block(content: &str, block_name: &str) -> Option<String> {
    let header = format!("{block_name}:");
    let mut started = false;
    let mut out = Vec::new();

    for line in content.lines() {
        let is_top_level = !line.starts_with(' ') && !line.starts_with('\t');
        if !started {
            if line.trim_start().starts_with(&header) {
                started = true;
                out.push(line.to_string());
            }
            continue;
        }

        if is_top_level && !line.trim().is_empty() && line.split_once(':').is_some() {
            break;
        }
        out.push(line.to_string());
    }

    if out.is_empty() {
        None
    } else {
        Some(format!("{}\n", out.join("\n")))
    }
}

fn top_level_block_names(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            if line.starts_with(' ') || line.starts_with('\t') {
                return None;
            }
            let (name, _) = line.split_once(':')?;
            if name.is_empty() {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

fn strip_job_rules(block: &str) -> String {
    let mut out = Vec::new();
    let mut skipping_rules = false;

    for line in block.lines() {
        let indent = line.chars().take_while(|ch| *ch == ' ').count();
        if indent == 2 && line.trim() == "rules:" {
            skipping_rules = true;
            continue;
        }
        if skipping_rules {
            if !line.trim().is_empty() && indent <= 2 {
                skipping_rules = false;
            } else {
                continue;
            }
        }
        out.push(line.to_string());
    }

    format!("{}\n", out.join("\n"))
}

fn collect_ci_blocks(
    workspace: &Path,
) -> (Vec<String>, std::collections::BTreeMap<String, String>) {
    let mut hidden = Vec::new();
    let mut jobs = std::collections::BTreeMap::new();
    let ci_dir = workspace.join("ci/gitlab");
    let Ok(entries) = std::fs::read_dir(ci_dir) else {
        return (hidden, jobs);
    };

    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("yml") {
            paths.push(path);
        }
    }
    paths.sort();

    for path in paths {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for name in top_level_block_names(&content) {
            if let Some(block) = extract_top_level_yaml_block(&content, &name) {
                if name.starts_with('.') {
                    hidden.push(block);
                } else {
                    jobs.entry(name).or_insert(block);
                }
            }
        }
    }

    (hidden, jobs)
}

fn emit_child_plan_context(plan: &ExternalTestPlan) -> String {
    let changed_paths = plan.changed_paths.join("\n");
    let selected_jobs = materialized_jobs(plan);
    let selected_json = serde_json::to_string(&selected_jobs).unwrap_or_else(|_| "[]".to_string());
    let plan_json = format!(
        "{{\"mode\":\"{}\",\"selected_jobs\":{selected_json}}}",
        match plan.mode {
            ExternalPlanMode::Full => "full",
            ExternalPlanMode::Selected => "selected",
            ExternalPlanMode::DocsOnly => "docs_only",
        }
    );
    let skipped_json = serde_json::to_string(&explain_external_skipped_json(plan))
        .unwrap_or_else(|_| "{}".to_string());
    let heredoc_body = |value: &str| {
        value
            .lines()
            .map(|line| format!("         \x20     {line}\n"))
            .collect::<String>()
    };
    format!(
        "plan-tests:\n\
         \x20 stage: lint\n\
         \x20 image: alpine:3.20\n\
         \x20 tags: [default]\n\
         \x20 script:\n\
         \x20   - mkdir -p target/jeryu\n\
         \x20   - |\n\
         \x20     cat > target/jeryu/changed-files.txt <<'VTI_CHANGED_FILES'\n\
{changed_paths}         \x20     VTI_CHANGED_FILES\n\
         \x20   - |\n\
         \x20     cat > target/jeryu/vti-plan.json <<'VTI_PLAN_JSON'\n\
{plan_json}         \x20     VTI_PLAN_JSON\n\
         \x20   - |\n\
         \x20     cat > target/jeryu/vti-skipped.json <<'VTI_SKIPPED_JSON'\n\
{skipped_json}         \x20     VTI_SKIPPED_JSON\n\
         \x20 artifacts:\n\
         \x20   when: always\n\
         \x20   expire_in: 7 days\n\
         \x20   paths:\n\
         \x20     - target/jeryu/changed-files.txt\n\
         \x20     - target/jeryu/vti-plan.json\n\
         \x20     - target/jeryu/vti-skipped.json\n\n",
        changed_paths = heredoc_body(&changed_paths),
        plan_json = heredoc_body(&plan_json),
        skipped_json = heredoc_body(&skipped_json),
    )
}

/// Generate GitLab child pipeline YAML from an external test plan.
pub fn emit_external_gitlab_yaml(plan: &ExternalTestPlan, workspace: Option<&Path>) -> String {
    match &plan.mode {
        ExternalPlanMode::DocsOnly => {
            let comment = match &plan.mode {
                ExternalPlanMode::DocsOnly => "# VTI: docs-only — no tests required",
                _ => panic!("docs-only branch selected for non-docs-only mode"),
            };
            format!(
                "{comment}\n\
                 stages:\n  - noop\n\n\
                 vti-noop:\n\
                 \x20 stage: noop\n\
                 \x20 script: [\"echo 'VTI: {mode}'\"]\n",
                mode = match &plan.mode {
                    ExternalPlanMode::DocsOnly => "docs-only",
                    _ => panic!("docs-only branch selected for non-docs-only mode"),
                }
            )
        }
        ExternalPlanMode::Full | ExternalPlanMode::Selected => {
            let mut yaml = String::new();
            yaml.push_str("# Auto-generated by jeryu Test Intelligence.\n");
            yaml.push_str("# This child pipeline materializes only the VTI-selected graph.\n");
            yaml.push_str("# Do not run this file directly.\n\n");
            yaml.push_str("variables:\n");
            yaml.push_str("  CI_PIPELINE_PRODUCT: \"main-candidate\"\n");
            yaml.push_str("  VTI_FORCE_SELECTED_GRAPH: \"1\"\n");
            yaml.push_str("  VTI_STATIC_MAIN: \"1\"\n");
            yaml.push_str(&format!(
                "  VTI_SELECTED_JOBS: \",{},\"\n\n",
                materialized_jobs(plan).join(",")
            ));
            yaml.push_str(
                "stages:\n  - lint\n  - compile\n  - package\n  - test-rust\n  - test-tools\n  - test-shell\n  - test-security\n  - test-e2e\n  - audit\n  - audit-seed-data\n  - deploy\n  - report\n\n",
            );
            yaml.push_str(&emit_child_plan_context(plan));

            let Some(workspace) = workspace else {
                for job in materialized_jobs(plan) {
                    yaml.push_str(&format!(
                        "{job}:\n  stage: test-rust\n  image: rust:1.92.0\n  tags: [build]\n  script:\n    - cargo run -p veox-testctl -- ci-job {job}\n\n"
                    ));
                }
                return yaml;
            };

            let (hidden_blocks, job_blocks) = collect_ci_blocks(workspace);
            for block in hidden_blocks {
                yaml.push_str(&block);
                yaml.push('\n');
            }

            for job in materialized_jobs(plan) {
                if job == "plan-tests" {
                    continue;
                }
                if let Some(block) = job_blocks.get(&job) {
                    yaml.push_str(&strip_job_rules(block));
                    yaml.push('\n');
                } else {
                    yaml.push_str(&format!(
                        "{job}:\n  stage: test-rust\n  image: rust:1.92.0\n  tags: [build]\n  script:\n    - cargo run -p veox-testctl -- ci-job {job}\n\n"
                    ));
                }
            }

            yaml
        }
    }
}

/// Human-readable explanation of an external plan.
pub fn explain_external_plan(plan: &ExternalTestPlan) -> String {
    let mut out = String::new();
    let mode_label = match &plan.mode {
        ExternalPlanMode::Full => "FULL (all jobs)",
        ExternalPlanMode::Selected => "SELECTED (targeted jobs)",
        ExternalPlanMode::DocsOnly => "DOCS-ONLY (no tests)",
    };
    out.push_str("╭─ jeryu Test Intelligence Plan (external) ─────╮\n");
    out.push_str(&format!("│ Mode: {:<40} │\n", mode_label));
    out.push_str(&format!("│ Confidence: {:<34.2} │\n", plan.confidence));
    out.push_str("╰───────────────────────────────────────────────╯\n\n");

    if !plan.changed_paths.is_empty() {
        out.push_str("Changed:\n");
        for p in &plan.changed_paths {
            out.push_str(&format!("  ● {}\n", p));
        }
        out.push('\n');
    }

    if !plan.selected_jobs.is_empty() {
        out.push_str("Selected jobs:\n");
        for job in &plan.selected_jobs {
            out.push_str(&format!("  ✓ {}\n", job));
        }
        out.push('\n');
    }

    if !plan.skipped_jobs.is_empty() {
        out.push_str("Skipped jobs:\n");
        for job in &plan.skipped_jobs {
            out.push_str(&format!("  ○ {}\n", job));
        }
        out.push('\n');
    }

    if !plan.rationale.is_empty() {
        out.push_str("Rationale:\n");
        for reason in &plan.rationale {
            out.push_str(&format!("  → {}\n", reason));
        }
        out.push('\n');
    }

    if let Some(reason) = &plan.fallback_reason {
        out.push_str(&format!("Recovery: {}\n", reason));
    }

    out.push_str(&format!(
        "Summary: {} jobs selected, {} jobs skipped, {} subsystems affected\n",
        plan.selected_jobs.len(),
        plan.skipped_jobs.len(),
        plan.affected_subsystems.len()
    ));

    out
}

/// JSON representation of an external plan.
pub fn explain_external_json(plan: &ExternalTestPlan) -> serde_json::Value {
    serde_json::json!({
        "mode": match &plan.mode {
            ExternalPlanMode::Full => "full",
            ExternalPlanMode::Selected => "selected",
            ExternalPlanMode::DocsOnly => "docs_only",
        },
        "confidence": plan.confidence,
        "selected_jobs": plan.selected_jobs,
        "skipped_jobs": plan.skipped_jobs,
        "affected_subsystems": plan.affected_subsystems,
        "rationale": plan.rationale,
        "changed_paths": plan.changed_paths,
        "fallback_reason": plan.fallback_reason,
    })
}

/// JSON metadata for jobs that VTI intentionally omitted from the graph.
pub fn explain_external_skipped_json(plan: &ExternalTestPlan) -> serde_json::Value {
    let materialized: BTreeSet<String> = materialized_jobs(plan).into_iter().collect();
    let skipped_jobs: Vec<String> = match plan.mode {
        ExternalPlanMode::Full => Vec::new(),
        ExternalPlanMode::Selected | ExternalPlanMode::DocsOnly => plan
            .skipped_jobs
            .iter()
            .filter(|job| !materialized.contains(*job))
            .cloned()
            .collect(),
    };

    serde_json::json!({
        "mode": match &plan.mode {
            ExternalPlanMode::Full => "full",
            ExternalPlanMode::Selected => "selected",
            ExternalPlanMode::DocsOnly => "docs_only",
        },
        "status": "vti-skipped",
        "skipped_jobs": skipped_jobs,
        "materialized_jobs": materialized.into_iter().collect::<Vec<_>>(),
        "reason": plan.fallback_reason,
        "affected_subsystems": plan.affected_subsystems,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_map() -> TestMap {
        toml::from_str(
            r#"
[policy]
full_on_unknown = true
min_confidence = 0.85

[global_invalidators]
paths = ["Cargo.lock", "Cargo.toml", ".gitlab-ci.yml"]

[docs]
paths = ["*.md", "docs/**"]

[[subsystem]]
id = "app-core"
description = "Core application"
paths = ["apps/core/**"]
ci_jobs = ["compile", "test-core"]
cross_cutting = false

[[subsystem]]
id = "app-deploy"
description = "Deploy tooling"
paths = ["apps/deploy/**"]
ci_jobs = ["compile", "test-deploy"]
cross_cutting = false

[[subsystem]]
id = "shared-lib"
description = "Shared library"
paths = ["crates/shared/**"]
ci_jobs = ["compile", "test-core", "test-deploy"]
cross_cutting = true
"#,
        )
        .unwrap()
    }

    #[test]
    fn empty_diff_is_full() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &[]);
        assert_eq!(plan.mode, ExternalPlanMode::Full);
    }

    #[test]
    fn global_invalidator_triggers_full() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["Cargo.lock".into()]);
        assert_eq!(plan.mode, ExternalPlanMode::Full);
        assert!(plan.rationale[0].contains("global invalidator"));
    }

    #[test]
    fn docs_only_change() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["README.md".into(), "docs/setup.md".into()]);
        assert_eq!(plan.mode, ExternalPlanMode::DocsOnly);
    }

    #[test]
    fn single_subsystem_selects_jobs() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["apps/core/main.rs".into()]);
        assert_eq!(plan.mode, ExternalPlanMode::Selected);
        assert!(plan.selected_jobs.contains(&"test-core".into()));
        assert!(!plan.selected_jobs.contains(&"test-deploy".into()));
        assert_eq!(plan.affected_subsystems, vec!["app-core"]);
    }

    #[test]
    fn multiple_subsystems_union_jobs() {
        let m = test_map();
        let plan = plan_from_testmap(
            &m,
            &["apps/core/main.rs".into(), "apps/deploy/run.rs".into()],
        );
        assert_eq!(plan.mode, ExternalPlanMode::Selected);
        assert!(plan.selected_jobs.contains(&"test-core".into()));
        assert!(plan.selected_jobs.contains(&"test-deploy".into()));
    }

    #[test]
    fn deploy_core_changes_select_release_artifact_chain() {
        let m: TestMap = toml::from_str(
            r#"
[policy]
full_on_unknown = true
min_confidence = 0.85

[global_invalidators]
paths = [".jeryu/**", "ci/gitlab/**"]

[[subsystem]]
id = "deploy-crates"
paths = ["crates/deploy-core/**", "crates/deploy-manifest/**", "crates/deploy-doctor/**"]
ci_jobs = [
    "compile-workspace",
    "test-rust-veox-deploy",
    "build-release-artifacts",
    "build-bootstrap-musl",
    "build-enclave-server",
    "test-live-public-surface",
    "test-local-built",
    "publish-rc-dry-run",
    "test-local-rc",
]
"#,
        )
        .unwrap();
        let plan = plan_from_testmap(&m, &["crates/deploy-core/src/docker.rs".into()]);
        assert_eq!(plan.mode, ExternalPlanMode::Selected);
        for job in [
            "build-release-artifacts",
            "build-bootstrap-musl",
            "build-enclave-server",
            "test-live-public-surface",
            "test-local-built",
            "publish-rc-dry-run",
            "test-local-rc",
        ] {
            assert!(
                plan.selected_jobs.contains(&job.to_string()),
                "missing selected job {job}"
            );
        }
    }

    #[test]
    fn cross_cutting_lowers_confidence() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["crates/shared/lib.rs".into()]);
        assert_eq!(plan.mode, ExternalPlanMode::Selected);
        assert!(plan.confidence < 1.0, "should be < 1.0 for cross-cutting");
        assert_eq!(plan.confidence, 0.90);
    }

    #[test]
    fn unknown_file_triggers_full() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["some/unknown/path.rs".into()]);
        assert_eq!(plan.mode, ExternalPlanMode::Full);
        assert!(plan.rationale.iter().any(|r| r.contains("unmatched")));
    }

    #[test]
    fn docs_mixed_with_code_not_docs_only() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["README.md".into(), "apps/core/main.rs".into()]);
        assert_eq!(plan.mode, ExternalPlanMode::Selected);
        assert!(plan.selected_jobs.contains(&"test-core".into()));
    }

    #[test]
    fn yaml_generation_selected() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["apps/core/main.rs".into()]);
        let yaml = emit_external_gitlab_yaml(&plan, None);
        assert!(yaml.contains("compile:"));
        assert!(yaml.contains("test-core:"));
        assert!(yaml.contains("VTI_FORCE_SELECTED_GRAPH"));
        assert!(yaml.contains("ci-job test-core"));
    }

    #[test]
    fn yaml_generation_docs() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["README.md".into()]);
        let yaml = emit_external_gitlab_yaml(&plan, None);
        assert!(yaml.contains("vti-noop:"));
        assert!(yaml.contains("docs-only"));
    }

    #[test]
    fn explain_json_roundtrips() {
        let m = test_map();
        let plan = plan_from_testmap(&m, &["apps/core/main.rs".into()]);
        let json = explain_external_json(&plan);
        assert_eq!(json["mode"], "selected");
        assert!(!json["selected_jobs"].as_array().unwrap().is_empty());
    }

    #[test]
    fn unknown_not_full_when_policy_allows() {
        let m: TestMap = toml::from_str(
            r#"
[policy]
full_on_unknown = false
min_confidence = 0.50

[global_invalidators]
paths = ["Cargo.lock"]

[[subsystem]]
id = "app"
paths = ["src/**"]
ci_jobs = ["test"]
"#,
        )
        .unwrap();
        // With full_on_unknown = false, unmatched paths don't trigger full
        let plan = plan_from_testmap(&m, &["unknown/path.rs".into()]);
        // No subsystem matched, and no full-on-unknown, so selected with 0 jobs
        // But with unmatched penalty it might dip below confidence...
        // Actually with no matched subsystems and some unmatched, confidence = 1.0 - 0.15 = 0.85
        // min_confidence = 0.50, so this stays at Selected with 0 jobs
        assert_ne!(plan.mode, ExternalPlanMode::Full);
    }
}
