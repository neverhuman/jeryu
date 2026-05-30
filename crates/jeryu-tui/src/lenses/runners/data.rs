//! Runners lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`RunnersLensInput`].
//! No I/O. Per-node rows project from the read model's [`RunnersDashboard`]
//! items; live per-node telemetry (storage probes etc.) belongs to the daemon /
//! assembler, so the standalone lens reads only the contract.

use jeryu_readmodel::{HealthLevel, RunnersItem, RunnersSummary, TuiReadModel};

/// One runner-node row in the fleet grid.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerNodeRow {
    pub label: String,
    pub runner_id: String,
    pub pool: String,
    pub status: HealthLevel,
    pub tags: Vec<String>,
    /// Human-friendly last-contact, derived from `last_seen` presence.
    pub last_contact: String,
}

impl RunnerNodeRow {
    fn from_item(item: &RunnersItem) -> Self {
        Self {
            label: item.label.clone(),
            runner_id: item.runner_id.clone(),
            pool: item.pool.clone(),
            status: item.status,
            tags: item.tags.clone(),
            last_contact: match item.last_seen {
                Some(_) => "seen".into(),
                None => "—".into(),
            },
        }
    }

    /// Low-noise status word: stuck > busy > idle > online.
    ///
    /// `HealthLevel::Critical` reads as a stuck node (online but not making
    /// progress); `Degraded` reads as busy/saturated; `Warning` as idle.
    pub fn status_word(&self) -> &'static str {
        match self.status {
            HealthLevel::Critical => "STUCK",
            HealthLevel::Degraded => "busy",
            HealthLevel::Warning => "idle",
            HealthLevel::Healthy => "online",
            HealthLevel::Unknown => "probing",
        }
    }

    /// True when this node needs attention (stuck or unreachable/probing).
    pub fn is_alerting(&self) -> bool {
        matches!(self.status, HealthLevel::Critical)
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunnersLensInput {
    pub active_runners: u32,
    pub total_runners: u32,
    pub paused_runners: u32,
    pub draining_runners: u32,
    pub nodes: Vec<RunnerNodeRow>,
    pub event_cursor: u64,
}

impl RunnersLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.runners.summary.clone().unwrap_or(RunnersSummary {
            total_runners: model.mission.total_runners,
            active_runners: model.mission.active_runners,
            paused_runners: 0,
            draining_runners: 0,
        });
        let nodes: Vec<RunnerNodeRow> = model
            .runners
            .items
            .iter()
            .map(RunnerNodeRow::from_item)
            .collect();
        Self {
            active_runners: summary.active_runners,
            total_runners: summary.total_runners,
            paused_runners: summary.paused_runners,
            draining_runners: summary.draining_runners,
            nodes,
            event_cursor: model.event_cursor,
        }
    }

    /// Fleet utilization as whole-percent (0 when no runners are known).
    pub fn utilization_pct(&self) -> u32 {
        if self.total_runners == 0 {
            0
        } else {
            (self.active_runners as f64 / self.total_runners as f64 * 100.0).round() as u32
        }
    }

    /// Count of nodes flagged stuck — drives the fleet alert banner.
    pub fn stuck_nodes(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_alerting()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{RunnersDashboard, sample_read_model};

    #[test]
    fn from_default_read_model_has_no_nodes() {
        let input = RunnersLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.active_runners, 0);
        assert_eq!(input.total_runners, 0);
        assert!(input.nodes.is_empty());
        assert_eq!(input.utilization_pct(), 0);
        assert_eq!(input.stuck_nodes(), 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn projects_summary_and_nodes_from_dashboard() {
        let model = sample_read_model();
        let input = RunnersLensInput::from_read_model(&model);
        assert_eq!(input.total_runners, 8);
        assert_eq!(input.active_runners, 6);
        assert_eq!(input.paused_runners, 1);
        assert_eq!(input.draining_runners, 1);
        assert_eq!(input.nodes.len(), 1);
        assert_eq!(input.nodes[0].label, "oci-runner-1");
        assert_eq!(input.utilization_pct(), 75);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn status_word_classifies_health_levels() {
        let mut row = RunnerNodeRow {
            label: "n".into(),
            runner_id: "r".into(),
            pool: "trusted".into(),
            status: HealthLevel::Healthy,
            tags: vec![],
            last_contact: "seen".into(),
        };
        assert_eq!(row.status_word(), "online");
        row.status = HealthLevel::Degraded;
        assert_eq!(row.status_word(), "busy");
        row.status = HealthLevel::Warning;
        assert_eq!(row.status_word(), "idle");
        row.status = HealthLevel::Critical;
        assert_eq!(row.status_word(), "STUCK");
        assert!(row.is_alerting());
    }

    #[test]
    fn falls_back_to_mission_counts_without_summary() {
        let mut model = TuiReadModel::default();
        model.mission.active_runners = 2;
        model.mission.total_runners = 5;
        model.runners = RunnersDashboard::default();
        let input = RunnersLensInput::from_read_model(&model);
        assert_eq!(input.active_runners, 2);
        assert_eq!(input.total_runners, 5);
        assert_eq!(input.utilization_pct(), 40);
    }

    #[test]
    fn stuck_nodes_counts_critical() {
        let mut model = TuiReadModel::default();
        model.runners.items = vec![
            jeryu_readmodel::RunnersItem {
                id: "1".into(),
                label: "n1".into(),
                runner_id: "r1".into(),
                pool: "trusted".into(),
                status: HealthLevel::Critical,
                tags: vec![],
                last_seen: None,
            },
            jeryu_readmodel::RunnersItem {
                id: "2".into(),
                label: "n2".into(),
                runner_id: "r2".into(),
                pool: "trusted".into(),
                status: HealthLevel::Healthy,
                tags: vec![],
                last_seen: None,
            },
        ];
        let input = RunnersLensInput::from_read_model(&model);
        assert_eq!(input.stuck_nodes(), 1);
    }
}
