use serde::{Deserialize, Serialize};

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
