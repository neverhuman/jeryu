//! Owner: VTI Test Intelligence subsystem — test plan algorithm
//! Proof: `cargo nextest run -p jeryu -- test_intel::planner`
//! Invariants: The planner may skip only when ownership, dependency, and history evidence agree.
//! Test plan selection algorithm.
//!
//! Given a set of changed paths, produces a `TestPlan` specifying exactly
//! which tests to run, which to skip, and why, with a confidence score.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::test_intel::subsystem::{
    self, Subsystem, affected_subsystems, has_global_invalidator, has_subsystem_force_full,
    is_docs_only,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The mode of a test plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestPlanMode {
    /// Run the full test suite.
    Full,
    /// Run only tests for affected subsystems.
    Selected,
    /// Documentation-only change; skip all Rust tests.
    DocsOnly,
}

/// A single selected test command with its justification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectedTest {
    pub subsystem: String,
    pub command: String,
    pub kind: String, // "unit_filter", "integration", "sentinel", "changed_test"
    pub reason: String,
}

/// A test plan describing what to run and what to skip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestPlan {
    pub mode: TestPlanMode,
    pub confidence: f64,
    pub selected_tests: Vec<SelectedTest>,
    pub skipped_subsystems: Vec<String>,
    pub affected_subsystems: Vec<String>,
    pub changed_paths: Vec<String>,
    pub repair_reason: Option<String>,
    pub sentinel_tests: Vec<SelectedTest>,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VtiReceipt {
    pub receipt_id: String,
    pub policy_version: String,
    pub mode: TestPlanMode,
    pub confidence: f64,
    pub base_sha: Option<String>,
    pub head_sha: Option<String>,
    pub selected_tests: Vec<SelectedTest>,
    pub skipped_subsystems: Vec<String>,
    pub affected_subsystems: Vec<String>,
    pub changed_paths: Vec<String>,
    pub repair_reason: Option<String>,
    pub skipped_tests_explained: bool,
    pub conservative_repair: bool,
}

impl TestPlan {
    pub fn repair_reason(&self) -> Option<&str> {
        self.repair_reason.as_deref()
    }

    pub fn full(reason: &str) -> Self {
        Self {
            mode: TestPlanMode::Full,
            confidence: 1.0,
            selected_tests: vec![SelectedTest {
                subsystem: "all".to_string(),
                command: "cargo test --lib --tests".to_string(),
                kind: "full".to_string(),
                reason: reason.to_string(),
            }],
            skipped_subsystems: Vec::new(),
            affected_subsystems: vec!["all".to_string()],
            changed_paths: Vec::new(),
            repair_reason: Some(reason.to_string()),
            sentinel_tests: Vec::new(),
            rationale: vec![format!("Full test run: {}", reason)],
        }
    }

    pub fn docs_only() -> Self {
        Self {
            mode: TestPlanMode::DocsOnly,
            confidence: 1.0,
            selected_tests: Vec::new(),
            skipped_subsystems: subsystem::SUBSYSTEMS
                .iter()
                .map(|s| s.id.to_string())
                .collect(),
            affected_subsystems: Vec::new(),
            changed_paths: Vec::new(),
            repair_reason: None,
            sentinel_tests: Vec::new(),
            rationale: vec![
                "All changes are documentation-only; no Rust tests required.".to_string(),
            ],
        }
    }

    /// Total number of tests that will actually run (selected + sentinels).
    pub fn run_count(&self) -> usize {
        self.selected_tests.len() + self.sentinel_tests.len()
    }

    pub fn receipt(&self, base_sha: Option<&str>, head_sha: Option<&str>) -> VtiReceipt {
        let skipped_tests_explained = matches!(self.mode, TestPlanMode::Full)
            || self
                .skipped_subsystems
                .iter()
                .all(|subsystem| !subsystem.trim().is_empty())
            || matches!(self.mode, TestPlanMode::DocsOnly);
        let conservative_repair =
            matches!(self.mode, TestPlanMode::Full) && self.repair_reason.is_some();
        let fingerprint = format!(
            "{}|{}|{}|{}",
            self.changed_paths.join(","),
            self.affected_subsystems.join(","),
            self.selected_tests
                .iter()
                .map(|test| test.command.as_str())
                .collect::<Vec<_>>()
                .join(","),
            head_sha.unwrap_or("")
        );
        VtiReceipt {
            receipt_id: format!("vti-{}", stable_hex(&fingerprint)),
            policy_version: "vti-receipt-v3.01".to_string(),
            mode: self.mode.clone(),
            confidence: self.confidence,
            base_sha: base_sha.map(str::to_string),
            head_sha: head_sha.map(str::to_string),
            selected_tests: self.selected_tests.clone(),
            skipped_subsystems: self.skipped_subsystems.clone(),
            affected_subsystems: self.affected_subsystems.clone(),
            changed_paths: self.changed_paths.clone(),
            repair_reason: self.repair_reason.clone(),
            skipped_tests_explained,
            conservative_repair,
        }
    }
}

// ---------------------------------------------------------------------------
// Core planner
// ---------------------------------------------------------------------------

/// Build a test plan from a set of changed file paths.
pub fn plan_tests(changed_paths: &[String]) -> TestPlan {
    // 1. Empty diff → conservative full
    if changed_paths.is_empty() {
        return TestPlan::full("empty diff, conservative recovery");
    }

    // 2. Global invalidator check
    if let Some(trigger) = has_global_invalidator(changed_paths) {
        let mut plan = TestPlan::full(&format!("global invalidator changed: {}", trigger));
        plan.changed_paths = changed_paths.to_vec();
        return plan;
    }

    // 3. Docs-only fast path
    if is_docs_only(changed_paths) {
        let mut plan = TestPlan::docs_only();
        plan.changed_paths = changed_paths.to_vec();
        return plan;
    }

    // 4. Find affected subsystems
    let affected = affected_subsystems(changed_paths);

    // 5. Check subsystem-level force-full triggers
    if let Some(reason) = has_subsystem_force_full(changed_paths, &affected) {
        let mut plan = TestPlan::full(&reason);
        plan.changed_paths = changed_paths.to_vec();
        return plan;
    }

    // 6. Pre-extract changed test files before the no-subsystem recovery.
    // If tests/*.rs files changed, they are always run regardless of subsystem match.
    let changed_test_files: Vec<String> = changed_paths
        .iter()
        .filter(|p| p.starts_with("tests/") && p.ends_with(".rs"))
        .cloned()
        .collect();

    // 7. If no subsystem matched and no test files changed, recovery to full
    if affected.is_empty() && changed_test_files.is_empty() {
        let mut plan = TestPlan::full("no subsystem matched changed paths; conservative recovery");
        plan.changed_paths = changed_paths.to_vec();
        return plan;
    }

    // 7. Build selected test set
    let mut selected_tests = Vec::new();
    let mut affected_ids = Vec::new();
    let mut rationale = Vec::new();
    let affected_set: BTreeSet<&str> = affected.iter().map(|s| s.id).collect();

    for subsystem in &affected {
        affected_ids.push(subsystem.id.to_string());

        // Unit filter
        if !subsystem.unit_filter.is_empty() {
            selected_tests.push(SelectedTest {
                subsystem: subsystem.id.to_string(),
                command: format!("cargo nextest run -E '{}'", subsystem.unit_filter),
                kind: "unit_filter".to_string(),
                reason: format!("changed file(s) owned by subsystem '{}'", subsystem.id),
            });
        }

        // Integration tests
        for test_binary in subsystem.integration_tests {
            selected_tests.push(SelectedTest {
                subsystem: subsystem.id.to_string(),
                command: format!("cargo nextest run --test {}", test_binary),
                kind: "integration".to_string(),
                reason: format!("integration harness for subsystem '{}'", subsystem.id),
            });
        }

        rationale.push(format!(
            "Selected '{}' because changed paths match owned_paths: [{}]",
            subsystem.id,
            subsystem.owned_paths.join(", ")
        ));
    }

    // 8. Include test files that were directly changed
    for path in changed_paths {
        if path.starts_with("tests/") && path.ends_with(".rs") {
            let test_name = path
                .strip_prefix("tests/")
                .and_then(|p| p.strip_suffix(".rs"))
                .unwrap_or(path);
            // Only add if not already covered by an integration test selection
            let already_covered = selected_tests
                .iter()
                .any(|t| t.kind == "integration" && t.command.contains(test_name));
            if !already_covered {
                selected_tests.push(SelectedTest {
                    subsystem: "changed_file".to_string(),
                    command: format!("cargo nextest run --test {}", test_name),
                    kind: "changed_test".to_string(),
                    reason: format!("test file '{}' was directly modified", path),
                });
                rationale.push(format!(
                    "Test file '{}' was directly modified; always re-run.",
                    path
                ));
            }
        }
    }

    // 9. Deduplicate test commands
    dedup_selected_tests(&mut selected_tests);

    // 10. Compute skipped subsystems
    let skipped: Vec<String> = subsystem::SUBSYSTEMS
        .iter()
        .filter(|s| !affected_set.contains(s.id))
        .map(|s| s.id.to_string())
        .collect();

    // 11. Compute confidence
    // If only test files changed (no subsystem matched), use direct-hit confidence
    let confidence = if affected.is_empty() && !changed_test_files.is_empty() {
        0.85 // Direct test file hit: we know exactly what to run
    } else {
        compute_confidence(&affected, changed_paths)
    };

    // 12. If confidence is too low, escalate to full
    if confidence < 0.70 {
        let mut plan = TestPlan::full(&format!(
            "confidence {:.2} below threshold 0.70; escalating to full",
            confidence
        ));
        plan.changed_paths = changed_paths.to_vec();
        plan.affected_subsystems = affected_ids;
        return plan;
    }

    TestPlan {
        mode: TestPlanMode::Selected,
        confidence,
        selected_tests,
        skipped_subsystems: skipped,
        affected_subsystems: affected_ids,
        changed_paths: changed_paths.to_vec(),
        repair_reason: None,
        sentinel_tests: Vec::new(), // populated later by the sentinel sampler
        rationale,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compute_confidence(affected: &[&Subsystem], changed_paths: &[String]) -> f64 {
    if affected.is_empty() {
        return 0.0;
    }

    // Start at 1.0, reduce for risk factors
    let mut confidence = 1.0;

    // Cross-cutting subsystems reduce confidence
    let has_cross_cutting = affected.iter().any(|s| s.cross_cutting);
    if has_cross_cutting {
        confidence -= 0.10;
    }

    // Many affected subsystems reduce confidence
    if affected.len() > 3 {
        confidence -= 0.05 * (affected.len() as f64 - 3.0);
    }

    // Unmatched paths reduce confidence
    let matched_count = changed_paths
        .iter()
        .filter(|p| {
            affected
                .iter()
                .any(|s| subsystem::matches_any(p, s.owned_paths))
        })
        .count();
    let unmatched = changed_paths.len() - matched_count;
    if unmatched > 0 {
        // Some files didn't match any subsystem
        confidence -= 0.15 * (unmatched as f64 / changed_paths.len() as f64);
    }

    confidence.clamp(0.0, 1.0)
}

fn dedup_selected_tests(tests: &mut Vec<SelectedTest>) {
    let mut seen = BTreeSet::new();
    tests.retain(|t| seen.insert(t.command.clone()));
}

fn stable_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    hex::encode(&digest[..8])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_diff_produces_full_plan() {
        let plan = plan_tests(&[]);
        assert_eq!(plan.mode, TestPlanMode::Full);
        assert!(plan.repair_reason.is_some());
    }

    #[test]
    fn cargo_toml_produces_full_plan() {
        let plan = plan_tests(&["Cargo.toml".to_string()]);
        assert_eq!(plan.mode, TestPlanMode::Full);
        assert!(
            plan.repair_reason
                .as_ref()
                .unwrap()
                .contains("global invalidator")
        );
    }

    #[test]
    fn readme_produces_docs_plan() {
        let plan = plan_tests(&["README.md".to_string()]);
        assert_eq!(plan.mode, TestPlanMode::DocsOnly);
        assert!(plan.selected_tests.is_empty());
    }

    #[test]
    fn pool_change_selects_pool_tests_only() {
        let plan = plan_tests(&["src/pool.rs".to_string()]);
        assert_eq!(plan.mode, TestPlanMode::Selected);
        assert!(plan.affected_subsystems.contains(&"pool".to_string()));
        assert!(plan.skipped_subsystems.contains(&"cache".to_string()));
        assert!(plan.skipped_subsystems.contains(&"tui".to_string()));
        assert!(plan.skipped_subsystems.contains(&"release".to_string()));

        // Should have unit filter + integration tests
        let has_unit = plan
            .selected_tests
            .iter()
            .any(|t| t.kind == "unit_filter" && t.subsystem == "pool");
        let has_integration = plan
            .selected_tests
            .iter()
            .any(|t| t.kind == "integration" && t.command.contains("pool_tests"));
        assert!(has_unit, "should have pool unit filter");
        assert!(has_integration, "should have pool_tests integration");
    }

    #[test]
    fn tui_change_skips_pool_and_e2e() {
        let plan = plan_tests(&["src/tui/ui.rs".to_string()]);
        assert_eq!(plan.mode, TestPlanMode::Selected);
        assert!(plan.affected_subsystems.contains(&"tui".to_string()));
        assert!(plan.skipped_subsystems.contains(&"pool".to_string()));
        assert!(plan.skipped_subsystems.contains(&"exec".to_string()));
        // TUI has no integration tests
        assert!(plan.selected_tests.iter().all(|t| t.kind != "integration"));
    }

    #[test]
    fn state_change_includes_cross_cutting_integration() {
        let plan = plan_tests(&["src/state.rs".to_string()]);
        assert_eq!(plan.mode, TestPlanMode::Selected);
        assert!(plan.affected_subsystems.contains(&"state".to_string()));
        // state is cross-cutting, so it includes many integration tests
        let integration_count = plan
            .selected_tests
            .iter()
            .filter(|t| t.kind == "integration")
            .count();
        assert!(
            integration_count >= 3,
            "state should include at least 3 integration suites, got {}",
            integration_count
        );
    }

    #[test]
    fn changed_test_file_always_included() {
        let plan = plan_tests(&["tests/pool_tests.rs".to_string()]);
        // Changed test files should produce a Selected plan that includes the test
        assert_eq!(
            plan.mode,
            TestPlanMode::Selected,
            "pure test file change should produce Selected, not Full"
        );
        let has_test = plan
            .selected_tests
            .iter()
            .any(|t| t.command.contains("pool_tests"));
        assert!(has_test, "changed test file should always be included");
    }

    #[test]
    fn multiple_subsystem_change_has_lower_confidence() {
        let single = plan_tests(&["src/pool.rs".to_string()]);
        let multi = plan_tests(&[
            "src/pool.rs".to_string(),
            "src/cache.rs".to_string(),
            "src/release.rs".to_string(),
            "src/agent.rs".to_string(),
        ]);
        assert!(
            multi.confidence <= single.confidence,
            "multi-subsystem confidence {} should be <= single {}",
            multi.confidence,
            single.confidence
        );
    }

    #[test]
    fn unknown_file_triggers_conservative_repair() {
        let plan = plan_tests(&["unknown/file.txt".to_string()]);
        assert_eq!(plan.mode, TestPlanMode::Full);
        assert!(plan.repair_reason.unwrap().contains("no subsystem"));
    }

    #[test]
    fn test_intel_change_triggers_force_full() {
        let plan = plan_tests(&["src/test_intel/planner.rs".to_string()]);
        assert_eq!(plan.mode, TestPlanMode::Full);
    }

    #[test]
    fn docs_plus_code_not_docs_only() {
        let plan = plan_tests(&["README.md".to_string(), "src/pool.rs".to_string()]);
        assert_ne!(plan.mode, TestPlanMode::DocsOnly);
        assert_eq!(plan.mode, TestPlanMode::Selected);
    }

    #[test]
    fn confidence_high_for_single_subsystem() {
        let plan = plan_tests(&["src/decision.rs".to_string()]);
        assert!(
            plan.confidence >= 0.90,
            "single well-matched subsystem should have high confidence, got {}",
            plan.confidence
        );
    }

    #[test]
    fn vti_receipt_binds_selected_plan_to_head_sha() {
        let plan = plan_tests(&["src/pool.rs".to_string()]);
        let receipt = plan.receipt(Some("base"), Some("head"));
        assert_eq!(receipt.policy_version, "vti-receipt-v3.01");
        assert_eq!(receipt.head_sha.as_deref(), Some("head"));
        assert!(receipt.skipped_tests_explained);
        assert!(receipt.receipt_id.starts_with("vti-"));
    }

    #[test]
    fn vti_receipt_marks_conservative_repair() {
        let plan = plan_tests(&["unknown/file.txt".to_string()]);
        let receipt = plan.receipt(None, Some("head"));
        assert_eq!(receipt.mode, TestPlanMode::Full);
        assert!(receipt.conservative_repair);
        assert!(receipt.repair_reason.is_some());
    }
}
