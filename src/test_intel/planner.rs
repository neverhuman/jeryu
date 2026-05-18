//! Owner: VTI Test Intelligence subsystem — test plan algorithm
//! Proof: `cargo nextest run -p jeryu -- test_intel::planner`
//! Invariants: The planner may skip only when ownership, dependency, and history evidence agree.
//! Test plan selection algorithm.
//!
//! Given a set of changed paths, produces a `TestPlan` specifying exactly
//! which tests to run, which to skip, and why, with a confidence score.

use std::collections::BTreeSet;

use crate::test_intel::subsystem::{
    self, Subsystem, affected_subsystems, has_global_invalidator, has_subsystem_force_full,
    is_docs_only,
};

#[path = "planner_types.rs"]
mod types;
pub use types::{SelectedTest, TestPlan, TestPlanMode, VtiReceipt};

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;
