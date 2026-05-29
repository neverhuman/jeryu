//! Owner: Interactive TUI subsystem - Jankurai lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::jankurai::data`
//! Invariants: Pure projection from `JankuraiSnapshot` (`app.state.jankurai`)
//!             plus the selected entry index into an owned `JankuraiLensInput`.
//!             No I/O, no lifetimes — everything needed for rendering is cloned
//!             so the view never reaches back into app state.

use crate::tui::jankurai::{
    JankuraiDimension, JankuraiEntry, JankuraiHistoryPoint, JankuraiScan, JankuraiSnapshot,
};

/// Owned, render-ready projection of the Jankurai audit snapshot.
///
/// Mirrors every field the legacy `ui_panels_jankurai*` panel read out of
/// `app.state.jankurai` so the lens can reproduce the full audit view (summary,
/// status, score-history chart, dimension breakdown, caps/findings list, and the
/// selected-entry detail) without any further access to app state.
#[derive(Debug, Clone, Default)]
pub struct JankuraiLensInput {
    /// Whether the `jankurai` binary is installed (drives the status pane and
    /// the "not installed" empty state). Equivalent to the legacy
    /// `App::jankurai_available()`.
    pub installed: bool,
    /// Latest scan summary, if a `agent/repo-score.json` was parsed.
    pub last_scan: Option<JankuraiScan>,
    /// Score history points from `agent/score-history.jsonl`, oldest first.
    pub history: Vec<JankuraiHistoryPoint>,
    /// Per-dimension breakdown from the last scan.
    pub dimensions: Vec<JankuraiDimension>,
    /// Caps + findings recorded by the last scan.
    pub entries: Vec<JankuraiEntry>,
    /// Parse / load error, if the snapshot failed to refresh.
    pub error: Option<String>,
    /// Index of the highlighted entry in `entries` (clamped at render time).
    pub selected_index: usize,
}

impl JankuraiLensInput {
    /// Project `app.state.jankurai` (a [`JankuraiSnapshot`]) plus the
    /// currently-selected entry index into an owned input.
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

    /// The currently-selected entry, computed from `entries` + `selected_index`.
    ///
    /// Replaces the legacy `App::selected_jankurai_entry()`; the lens only
    /// receives state + index, so the selection is resolved locally.
    pub fn selected_entry(&self) -> Option<&JankuraiEntry> {
        self.entries.get(self.selected_index)
    }

    /// True once a scan has been parsed. Mirrors the legacy `available()`
    /// gating used to decide between the audit view and the empty state.
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
    fn from_state_preserves_selected_index() {
        let input = JankuraiLensInput::from_state(&JankuraiSnapshot::default(), 7);
        assert_eq!(input.selected_index, 7);
    }

    #[test]
    fn from_state_clones_snapshot_fields() {
        let snapshot = JankuraiSnapshot {
            installed: true,
            error: Some("boom".into()),
            ..Default::default()
        };
        let input = JankuraiLensInput::from_state(&snapshot, 0);
        assert!(input.installed);
        assert_eq!(input.error.as_deref(), Some("boom"));
    }

    #[test]
    fn selected_entry_resolves_from_index() {
        use crate::tui::jankurai::{JankuraiEntry, JankuraiEntryKind};
        let entry = JankuraiEntry {
            kind: JankuraiEntryKind::Finding,
            label: "first".into(),
            severity: Some("high".into()),
            hardness: Some("hard".into()),
            path: Some("src/lib.rs".into()),
            rule: Some("rule-a".into()),
            lane: Some("fast".into()),
            owner: Some("tools".into()),
            problem: Some("oops".into()),
            evidence: vec!["e1".into()],
            suggested_fix: Some("fix it".into()),
        };
        let snapshot = JankuraiSnapshot {
            installed: true,
            entries: vec![entry.clone(), entry],
            ..Default::default()
        };
        let input = JankuraiLensInput::from_state(&snapshot, 1);
        assert_eq!(input.selected_index, 1);
        assert!(input.selected_entry().is_some());
        assert_eq!(
            input.selected_entry().map(|e| e.label.as_str()),
            Some("first")
        );
    }
}
