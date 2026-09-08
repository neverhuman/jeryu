//! Owner: Interactive TUI subsystem - Jankurai lens data selector
//! Proof: `cargo test -p jeryu-tui --lib lenses::jankurai::data`
//! Invariants: Pure projection from `TuiReadModel` to `JankuraiLensInput`.
//!             No I/O. The Jankurai model types are embedded here (they are not
//!             yet exposed by the readmodel contract) so the lens is self-contained.

use chrono::{DateTime, Utc};
use jeryu_readmodel::TuiReadModel;

// ── Jankurai model types (embedded until readmodel exposes them) ──────────

#[derive(Debug, Clone, Default)]
pub struct JankuraiSnapshot {
    pub installed: bool,
    pub history: Vec<JankuraiHistoryPoint>,
    pub dimensions: Vec<JankuraiDimension>,
    pub entries: Vec<JankuraiEntry>,
    pub last_scan: Option<JankuraiScan>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JankuraiHistoryPoint {
    pub generated_at: DateTime<Utc>,
    pub score: i64,
    pub raw_score: Option<i64>,
    pub decision: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JankuraiScan {
    pub generated_at: Option<DateTime<Utc>>,
    pub score: i64,
    pub raw_score: i64,
    pub minimum_score: i64,
    pub decision: String,
    pub score_status: String,
    pub finding_count: usize,
    pub hard_findings: usize,
    pub soft_findings: usize,
    pub caps_applied: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct JankuraiDimension {
    pub name: String,
    pub weight: u64,
    pub score: u64,
    pub weighted_points: f64,
    pub evidence: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JankuraiEntryKind {
    Cap,
    Finding,
}

#[derive(Debug, Clone)]
pub struct JankuraiEntry {
    pub kind: JankuraiEntryKind,
    pub label: String,
    pub severity: Option<String>,
    pub hardness: Option<String>,
    pub path: Option<String>,
    pub rule: Option<String>,
    pub lane: Option<String>,
    pub owner: Option<String>,
    pub problem: Option<String>,
    pub evidence: Vec<String>,
    pub suggested_fix: Option<String>,
}

// ── Lens input ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct JankuraiLensInput {
    pub installed: bool,
    pub last_scan: Option<JankuraiScan>,
    pub history: Vec<JankuraiHistoryPoint>,
    pub dimensions: Vec<JankuraiDimension>,
    pub entries: Vec<JankuraiEntry>,
    pub error: Option<String>,
    pub selected_index: usize,
}

impl JankuraiLensInput {
    pub fn from_state(state: &JankuraiSnapshot, selected_index: usize) -> Self {
        Self {
            installed: state.installed,
            last_scan: state.last_scan.clone(),
            history: state.history.clone(),
            dimensions: state.dimensions.clone(),
            entries: state.entries.clone(),
            error: state.error.clone(),
            selected_index,
        }
    }

    /// Project a minimal summary from the read model (when no JankuraiSnapshot
    /// is available).
    pub fn from_read_model(_model: &TuiReadModel) -> Self {
        Self {
            installed: true,
            last_scan: None,
            history: Vec::new(),
            dimensions: Vec::new(),
            entries: Vec::new(),
            error: None,
            selected_index: 0,
        }
    }

    pub fn selected_entry(&self) -> Option<&JankuraiEntry> {
        self.entries.get(self.selected_index)
    }

    pub fn has_scan(&self) -> bool {
        self.last_scan.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_empty_state_yields_empty_input() {
        let input = JankuraiLensInput::from_state(&JankuraiSnapshot::default(), 0);
        assert!(!input.installed);
        assert!(input.last_scan.is_none());
        assert!(input.history.is_empty());
        assert!(input.dimensions.is_empty());
        assert!(input.entries.is_empty());
        assert!(input.error.is_none());
        assert_eq!(input.selected_index, 0);
        assert!(!input.has_scan());
        assert!(input.selected_entry().is_none());
    }

    #[test]
    fn from_read_model_produces_installed_empty() {
        let model = TuiReadModel::default();
        let input = JankuraiLensInput::from_read_model(&model);
        assert!(input.installed);
        assert!(!input.has_scan());
    }
}
