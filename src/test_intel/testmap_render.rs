use super::*;
use std::collections::BTreeSet;

#[path = "testmap_render_yaml.rs"]
mod yaml_gen;
pub use yaml_gen::emit_external_gitlab_yaml;

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

pub(crate) fn materialized_jobs(plan: &ExternalTestPlan) -> Vec<String> {
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

    if let Some(reason) = &plan.repair_reason {
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
        "repair_reason": plan.repair_reason,
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
        "reason": plan.repair_reason,
        "affected_subsystems": plan.affected_subsystems,
    })
}
