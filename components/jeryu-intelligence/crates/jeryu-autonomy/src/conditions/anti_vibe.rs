//! Anti-vibe deterministic detectors.
//!
//! These read only `EvidencePack` fields — never the network, filesystem, git,
//! or an LLM. They flag the change-shape patterns that vibe-coded PRs use to
//! slip past review: deleted/weakened tests, lowered coverage, mass snapshot
//! rewrites, and edits to release/deploy policy or agent-prompt/judge policy.

use super::HardStop;
use super::paths::any_path_matches;
use crate::types::{AgentApprovalReceipt, ChangedFile, EvidencePack};

const TEST_PATH_SUBSTRINGS: &[&str] = &[
    "/tests/",
    "/test/",
    "/__tests__/",
    "/spec/",
    ".test.",
    "_test.",
    ".spec.",
    "_spec.",
];

const RELEASE_DEPLOY_POLICY_PATHS: &[&str] = &[
    ".jeryu/autonomy/policies/release.yml",
    ".jeryu/autonomy/policies/freeze.yml",
    "agent/proof-lanes.toml",
    "proof-lanes.toml",
    "Justfile",
];

// R-7 (D1): the legacy external-host CI prefix is removed and replaced by the
// jeryu-native `.jeryu/ci/` prefix. `.github/...` plus generic infra prefixes
// survive.
const RELEASE_DEPLOY_PATH_PREFIXES: &[&str] = &[
    ".github/workflows/release",
    ".github/workflows/deploy",
    ".jeryu/ci/",
    "ops/ci/",
    "deploy/",
    "infra/",
    "k8s/",
    "helm/",
    "terraform/",
];

const PROMPT_OR_JUDGE_PREFIXES: &[&str] = &[
    ".jeryu/autonomy/prompts/",
    ".jeryu/autonomy/agents/",
    ".jeryu/autonomy/policies/approvals.yml",
    ".jeryu/autonomy/policies/risk.yml",
    ".jeryu/autonomy/policies/protected-paths.yml",
];

fn is_test_path(p: &str) -> bool {
    TEST_PATH_SUBSTRINGS.iter().any(|s| p.contains(s))
}

pub(super) fn cond_removes_or_weakens_tests(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let deletions: Vec<&ChangedFile> = p
        .changed_files
        .iter()
        .filter(|f| is_test_path(&f.path) && f.lines_removed > f.lines_added)
        .collect();
    if deletions.is_empty() {
        return None;
    }
    let total_removed: u32 = deletions
        .iter()
        .map(|f| f.lines_removed - f.lines_added)
        .sum();
    // Tolerate one small refactor; flag if ≥2 deletion files or ≥20 net lines gone.
    if deletions.len() < 2 && total_removed < 20 {
        return None;
    }
    Some(HardStop {
        name: "removes_or_weakens_tests".into(),
        reason: format!(
            "{} test file(s) lost a net {} line(s); fail-closed without an explicit \
             test-integrity reviewer pass",
            deletions.len(),
            total_removed
        ),
        details: serde_json::json!({
            "paths": deletions.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            "net_lines_removed": total_removed,
        }),
    })
}

pub(super) fn cond_coverage_threshold_lowered(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let delta = p.tests.coverage_delta?;
    if delta < -0.5 {
        Some(HardStop {
            name: "coverage_threshold_lowered".into(),
            reason: format!("coverage dropped {:.2}pp on this change", delta),
            details: serde_json::json!({ "coverage_delta": delta }),
        })
    } else {
        None
    }
}

pub(super) fn cond_snapshot_mass_replacement(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let mut snap_lines: u32 = 0;
    let mut snap_files: Vec<String> = vec![];
    for f in &p.changed_files {
        let is_snap = f.path.contains("__snapshots__/")
            || f.path.contains("/snapshots/")
            || f.path.ends_with(".snap")
            || f.path.ends_with(".snap.json")
            || f.path.ends_with(".snap.new");
        if is_snap {
            snap_lines = snap_lines.saturating_add(f.lines_added + f.lines_removed);
            snap_files.push(f.path.clone());
        }
    }
    if snap_lines < 200 {
        return None;
    }
    Some(HardStop {
        name: "snapshot_mass_replacement".into(),
        reason: format!(
            "{} snapshot file(s) changed by {} line(s); needs explicit rendered-diff justification",
            snap_files.len(),
            snap_lines
        ),
        details: serde_json::json!({ "paths": snap_files, "lines": snap_lines }),
    })
}

pub(super) fn cond_changes_release_or_deploy_policy(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    any_path_matches(p, RELEASE_DEPLOY_POLICY_PATHS, RELEASE_DEPLOY_PATH_PREFIXES).map(|paths| {
        HardStop {
            name: "changes_release_or_deploy_policy".into(),
            reason: "PR edits release/deploy policy or infra; require elevated review".into(),
            details: serde_json::json!({ "paths": paths }),
        }
    })
}

pub(super) fn cond_changes_agent_prompts_or_judge_policy(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    any_path_matches(p, &[], PROMPT_OR_JUDGE_PREFIXES).map(|paths| HardStop {
        name: "changes_agent_prompts_or_judge_policy".into(),
        reason: "PR edits agent prompts or judge policy; require elevated review".into(),
        details: serde_json::json!({ "paths": paths }),
    })
}
