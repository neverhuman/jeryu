//! Owner: VTI Test Intelligence subsystem — nightly oracle
//! Proof: `cargo nextest run -p jeryu -- test_intel::nightly`
//! Invariants: Nightly comparisons preserve full-suite evidence for calibrating skip safety.
//! Nightly Oracle — Self-healing test selector auditing.
//!
//! This module implements the nightly audit loop that validates VTI's test
//! selection accuracy. It compares the results of a nightly full test run
//! against what VTI would have selected, identifies selector misses (tests
//! that VTI would have skipped but actually failed), and records them for
//! subsystem rule improvement.

#[path = "nightly_types.rs"]
mod types;
pub use types::{AuditReport, LearnResult, SelectorMiss};

#[path = "nightly_helpers.rs"]
mod helpers;

#[cfg(test)]
#[path = "nightly_tests.rs"]
mod tests;

// ---------------------------------------------------------------------------
// Audit logic
// ---------------------------------------------------------------------------

/// Given a set of changed paths and the full nightly test results,
/// compute what VTI would have done and identify any misses.
pub fn audit_selector(
    changed_paths: &[String],
    failed_tests: &[String],
    all_tests: &[String],
    sha: &str,
    test_map: Option<&super::testmap::TestMap>,
) -> AuditReport {
    let (selected_commands, skipped_subsystems) = if let Some(map) = test_map {
        let plan = super::testmap::plan_from_testmap(map, changed_paths);
        (
            plan.selected_jobs,
            plan.skipped_jobs, // For testmap, skipped jobs maps best to skipped systems here
        )
    } else {
        use super::planner;
        let plan = planner::plan_tests(changed_paths);
        (
            plan.selected_tests.into_iter().map(|t| t.command).collect(),
            plan.skipped_subsystems,
        )
    };

    // Identify misses: tests that failed BUT were in a skipped subsystem
    let mut misses = Vec::new();

    for failed in failed_tests {
        // Check if this test would have been covered by a selected command
        let covered = selected_commands.iter().any(|cmd| {
            // Simple heuristic: if the test name appears in the command filter
            helpers::extract_test_patterns(cmd)
                .iter()
                .any(|pat| failed.contains(pat))
        });

        if !covered {
            // This is a miss — VTI would have skipped it
            let responsible = helpers::find_responsible_subsystem(failed);
            misses.push(SelectorMiss {
                missed_test: failed.clone(),
                responsible_subsystem: responsible,
                failed_sha: sha.to_string(),
                detected_by: "nightly".to_string(),
            });
        }
    }

    let total = all_tests.len().max(1);
    let accuracy = if failed_tests.is_empty() {
        1.0
    } else {
        1.0 - (misses.len() as f64 / total as f64)
    };

    AuditReport {
        nightly_sha: sha.to_string(),
        total_tests: all_tests.len(),
        failed_tests: failed_tests.to_vec(),
        vti_selected: selected_commands,
        vti_skipped: skipped_subsystems,
        misses,
        accuracy,
        full_run_clean: failed_tests.is_empty(),
    }
}

/// Learn from a pipeline's outcomes and suggest rule improvements.
pub fn learn_from_audit(report: &AuditReport) -> LearnResult {
    let mut flagged_subsystems = Vec::new();
    let mut suggestions = Vec::new();

    if report.misses.is_empty() {
        suggestions.push("No selector misses. VTI selection is accurate.".into());
        return LearnResult {
            processed: report.total_tests,
            new_misses: 0,
            flagged_subsystems,
            suggestions,
        };
    }

    // Group misses by responsible subsystem
    let mut miss_by_subsystem: std::collections::BTreeMap<String, Vec<&SelectorMiss>> =
        std::collections::BTreeMap::new();
    for miss in &report.misses {
        let key = miss
            .responsible_subsystem
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        miss_by_subsystem.entry(key).or_default().push(miss);
    }

    for (subsystem, misses) in &miss_by_subsystem {
        flagged_subsystems.push(subsystem.clone());
        suggestions.push(format!(
            "Subsystem '{}' missed {} test(s). Consider widening its owned_paths or adding cross-cutting flag.",
            subsystem,
            misses.len()
        ));
        for miss in misses {
            suggestions.push(format!(
                "  → missed test: '{}' (sha: {})",
                miss.missed_test,
                &miss.failed_sha[..8.min(miss.failed_sha.len())]
            ));
        }
    }

    if report.accuracy < 0.95 {
        suggestions.push(format!(
            "WARNING: VTI accuracy {:.1}% is below 95% threshold. Consider recovery to full until rules are fixed.",
            report.accuracy * 100.0
        ));
    }

    LearnResult {
        processed: report.total_tests,
        new_misses: report.misses.len(),
        flagged_subsystems,
        suggestions,
    }
}
