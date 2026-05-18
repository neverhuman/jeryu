use crate::test_intel::subsystem;
use serde::{Deserialize, Serialize};

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

pub(crate) fn stable_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    hex::encode(&digest[..8])
}
