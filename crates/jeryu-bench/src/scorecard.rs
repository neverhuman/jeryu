//! Scorecard and required win-condition evaluation.

use crate::models::ScenarioClass;
use crate::receipt::BenchmarkReceipt;

/// Required speedup for one scenario.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BenchmarkTarget {
    pub scenario: ScenarioClass,
    pub minimum_speedup: f64,
}

impl BenchmarkTarget {
    /// Phase 10 targets from the engineering spec.
    pub const fn phase10_targets() -> [Self; 6] {
        [
            Self {
                scenario: ScenarioClass::TrustedNoOpRustPr,
                minimum_speedup: 10.0,
            },
            Self {
                scenario: ScenarioClass::SmallWarmRustPr,
                minimum_speedup: 5.0,
            },
            Self {
                scenario: ScenarioClass::MediumAffectedCratePr,
                minimum_speedup: 3.0,
            },
            Self {
                scenario: ScenarioClass::LargeAffectedCratePr,
                minimum_speedup: 2.0,
            },
            Self {
                scenario: ScenarioClass::TestOnlySharded,
                minimum_speedup: 3.0,
            },
            Self {
                scenario: ScenarioClass::MergeQueueMedian,
                minimum_speedup: 2.0,
            },
        ]
    }
}

/// One scorecard row.
#[derive(Clone, Debug, PartialEq)]
pub struct ScorecardEntry {
    pub scenario: ScenarioClass,
    pub observed_speedup: f64,
    pub required_speedup: f64,
    pub passed: bool,
}

/// Published scorecard.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scorecard {
    pub entries: Vec<ScorecardEntry>,
}

impl Scorecard {
    /// Evaluate receipts against Phase 10 win conditions.
    pub fn from_receipts(receipts: &[BenchmarkReceipt]) -> Self {
        let mut entries = Vec::new();
        for target in BenchmarkTarget::phase10_targets() {
            let observed = receipts
                .iter()
                .filter(|receipt| receipt.scenario == target.scenario)
                .map(|receipt| receipt.speedup_vs_competitor)
                .fold(0.0_f64, f64::max);
            entries.push(ScorecardEntry {
                scenario: target.scenario,
                observed_speedup: observed,
                required_speedup: target.minimum_speedup,
                passed: observed + f64::EPSILON >= target.minimum_speedup,
            });
        }
        Self { entries }
    }

    /// True when every target passes and at least one entry exists.
    pub fn passed(&self) -> bool {
        !self.entries.is_empty() && self.entries.iter().all(|entry| entry.passed)
    }

    /// Render a small Markdown scorecard.
    pub fn to_markdown(&self) -> String {
        let mut out =
            String::from("| Scenario | Observed | Required | Pass |\n|---|---:|---:|---|\n");
        for entry in &self.entries {
            out.push_str(&format!(
                "| {} | {:.2}x | {:.2}x | {} |\n",
                entry.scenario.slug(),
                entry.observed_speedup,
                entry.required_speedup,
                if entry.passed { "yes" } else { "no" }
            ));
        }
        out
    }
}
