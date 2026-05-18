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

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single audit finding: a test that was skipped by VTI but failed in full.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectorMiss {
    /// The test ID (e.g., "cargo test --lib pool_tests")
    pub missed_test: String,
    /// The subsystem that should have caught this
    pub responsible_subsystem: Option<String>,
    /// The SHA where the failure was detected
    pub failed_sha: String,
    /// How this miss was detected
    pub detected_by: String,
}

/// Summary of a nightly audit run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    /// The SHA of the nightly full run
    pub nightly_sha: String,
    /// Total tests in the full run
    pub total_tests: usize,
    /// Tests that failed in the full run
    pub failed_tests: Vec<String>,
    /// What VTI would have selected for this SHA
    pub vti_selected: Vec<String>,
    /// What VTI would have skipped for this SHA
    pub vti_skipped: Vec<String>,
    /// Tests that VTI would have missed (failed + skipped)
    pub misses: Vec<SelectorMiss>,
    /// Overall VTI accuracy for this run
    pub accuracy: f64,
    /// Was the full run clean (all passed)?
    pub full_run_clean: bool,
}

/// Result of learning from a pipeline's test outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnResult {
    /// Number of test outcomes processed
    pub processed: usize,
    /// Number of new misses detected
    pub new_misses: usize,
    /// Subsystems that need attention
    pub flagged_subsystems: Vec<String>,
    /// Suggested actions
    pub suggestions: Vec<String>,
}

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
            extract_test_patterns(cmd)
                .iter()
                .any(|pat| failed.contains(pat))
        });

        if !covered {
            // This is a miss — VTI would have skipped it
            let responsible = find_responsible_subsystem(failed);
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract test name patterns from a nextest -E filter expression.
fn extract_test_patterns(command: &str) -> Vec<String> {
    // Pattern: 'test(/foo|bar|baz/)'
    let mut patterns = Vec::new();
    if let Some(start) = command.find("test(/") {
        let rest = &command[start + 6..];
        if let Some(end) = rest.find("/)") {
            let inner = &rest[..end];
            for part in inner.split('|') {
                let clean = part.trim().to_string();
                if !clean.is_empty() {
                    patterns.push(clean);
                }
            }
        }
    }
    if patterns.is_empty() && !command.is_empty() {
        // Recovery path: use the whole command as a pattern
        patterns.push(command.to_string());
    }
    patterns
}

/// Try to identify which subsystem should have owned a failed test.
fn find_responsible_subsystem(test_name: &str) -> Option<String> {
    use super::subsystem::SUBSYSTEMS;

    let test_lower = test_name.to_lowercase();
    for rule in SUBSYSTEMS {
        // Check if the subsystem's test command patterns match this test name
        let filter = rule.unit_filter;
        let patterns = extract_test_patterns(filter);
        if patterns
            .iter()
            .any(|p| test_lower.contains(&p.to_lowercase()))
        {
            return Some(rule.id.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_patterns_from_nextest_filter() {
        let patterns = extract_test_patterns("cargo nextest run -E 'test(/pool|docker|runner/)'");
        assert_eq!(patterns, vec!["pool", "docker", "runner"]);
    }

    #[test]
    fn extract_patterns_recovery() {
        let patterns = extract_test_patterns("cargo test --lib");
        assert_eq!(patterns, vec!["cargo test --lib"]);
    }

    #[test]
    fn find_subsystem_for_pool_test() {
        let sub = find_responsible_subsystem("pool_connection_test");
        assert_eq!(sub, Some("pool".to_string()));
    }

    #[test]
    fn find_subsystem_for_cache_test() {
        let sub = find_responsible_subsystem("cache_eviction_test");
        assert_eq!(sub, Some("cache".to_string()));
    }
}
