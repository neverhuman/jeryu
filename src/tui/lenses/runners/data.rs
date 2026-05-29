//! Owner: Interactive TUI subsystem - Runners lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::runners::data`
//! Invariants: Pure projection to `RunnersLensInput`. No I/O. Per-node rows are
//!             projected from the live `NodeSummary` fleet (synced from the DB in
//!             `app_runtime_sync`), so the lens shows real multi-node runner
//!             health without any extra backend wiring.

use crate::api::read_model::TuiReadModel;
use crate::tui::app::NodeSummary;

/// One node row in the runners pane.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerNodeRow {
    pub alias: String,
    /// None = not probed this cycle; Some(true/false) = last probe result.
    pub reachable: Option<bool>,
    pub active_managers: usize,
    pub max_managers: usize,
    pub storage_used_gb: Option<f64>,
    pub storage_limit_gb: f64,
    pub last_contact_at: Option<String>,
}

impl RunnerNodeRow {
    fn from_summary(n: &NodeSummary) -> Self {
        Self {
            alias: n.alias.clone(),
            reachable: n.reachable,
            active_managers: n.active_managers,
            max_managers: n.max_managers,
            storage_used_gb: n.storage_used_gb,
            storage_limit_gb: n.storage_limit_gb,
            last_contact_at: n.last_contact_at.clone(),
        }
    }

    /// Low-noise status word: unreachable > saturated > idle > healthy.
    pub fn status(&self) -> &'static str {
        match self.reachable {
            Some(false) => "UNREACHABLE",
            None => "probing",
            Some(true) if self.max_managers > 0 && self.active_managers >= self.max_managers => {
                "saturated"
            }
            Some(true) if self.active_managers == 0 => "idle",
            Some(true) => "healthy",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunnersLensInput {
    pub active_runners: u32,
    pub total_runners: u32,
    pub event_cursor: u64,
    pub nodes: Vec<RunnerNodeRow>,
}

impl RunnersLensInput {
    /// Fixture/read-model path: counts only (no live node telemetry).
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            active_runners: model.mission.active_runners,
            total_runners: model.mission.total_runners,
            event_cursor: model.event_cursor,
            nodes: Vec::new(),
        }
    }

    /// Live path: project the per-node fleet from the synced `NodeSummary`
    /// list. Reserved/control nodes simply appear with their real counts;
    /// nothing is hidden.
    pub fn from_nodes(nodes: &[NodeSummary], event_cursor: u64) -> Self {
        let rows: Vec<RunnerNodeRow> = nodes.iter().map(RunnerNodeRow::from_summary).collect();
        let active: u32 = rows.iter().map(|n| n.active_managers as u32).sum();
        let total: u32 = rows.iter().map(|n| n.max_managers as u32).sum();
        Self {
            active_runners: active,
            total_runners: total,
            event_cursor,
            nodes: rows,
        }
    }

    /// Count of nodes whose last probe failed — drives the fleet alert banner.
    pub fn unreachable_nodes(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.reachable == Some(false))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(alias: &str, reachable: Option<bool>, active: usize, max: usize) -> NodeSummary {
        NodeSummary {
            alias: alias.into(),
            target: format!("deploy@{alias}"),
            enabled: true,
            reachable,
            active_managers: active,
            max_managers: max,
            storage_used_gb: Some(50.0),
            storage_limit_gb: 100.0,
            last_contact_at: Some("2m ago".into()),
        }
    }

    #[test]
    fn from_read_model_zero_has_no_nodes() {
        let input = RunnersLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.active_runners, 0);
        assert!(input.nodes.is_empty());
    }

    #[test]
    fn from_nodes_aggregates_counts_and_rows() {
        let nodes = vec![
            node("xbabe0", Some(true), 3, 4),
            node("xbabe1", Some(true), 1, 4),
            node("xbabe3", Some(false), 0, 4),
        ];
        let input = RunnersLensInput::from_nodes(&nodes, 42);
        assert_eq!(input.nodes.len(), 3);
        assert_eq!(input.active_runners, 4);
        assert_eq!(input.total_runners, 12);
        assert_eq!(input.event_cursor, 42);
        assert_eq!(input.unreachable_nodes(), 1);
    }

    #[test]
    fn node_status_classifies_states() {
        assert_eq!(
            RunnerNodeRow::from_summary(&node("n", Some(false), 0, 4)).status(),
            "UNREACHABLE"
        );
        assert_eq!(
            RunnerNodeRow::from_summary(&node("n", None, 0, 4)).status(),
            "probing"
        );
        assert_eq!(
            RunnerNodeRow::from_summary(&node("n", Some(true), 4, 4)).status(),
            "saturated"
        );
        assert_eq!(
            RunnerNodeRow::from_summary(&node("n", Some(true), 0, 4)).status(),
            "idle"
        );
        assert_eq!(
            RunnerNodeRow::from_summary(&node("n", Some(true), 2, 4)).status(),
            "healthy"
        );
    }
}
