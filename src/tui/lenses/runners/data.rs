//! Owner: Interactive TUI subsystem - Runners lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::runners::data`
//! Invariants: Pure projection from `TuiReadModel` to `RunnersLensInput`.
//!             No I/O.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct RunnersLensInput {
    pub active_runners: u32,
    pub total_runners: u32,
    pub degraded_runners: u32,
    pub orphaned_containers: u32,
    pub partial_inventory: bool,
    pub event_cursor: u64,
}

impl RunnersLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.runners.summary.as_ref();
        Self {
            active_runners: summary
                .map(|summary| summary.active_runners)
                .unwrap_or(model.mission.active_runners),
            total_runners: summary
                .map(|summary| summary.total_runners)
                .unwrap_or(model.mission.total_runners),
            degraded_runners: summary
                .map(|summary| summary.degraded_runners)
                .unwrap_or(model.system.runners.degraded),
            orphaned_containers: summary
                .map(|summary| summary.orphaned_containers)
                .unwrap_or(0),
            partial_inventory: summary
                .map(|summary| summary.partial_inventory)
                .unwrap_or(false),
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_zero() {
        let input = RunnersLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.active_runners, 0);
        assert_eq!(input.total_runners, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 99,
            ..Default::default()
        };
        let input = RunnersLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 99);
    }

    #[test]
    fn select_uses_runners_dashboard_summary_when_present() {
        let mut model = TuiReadModel::default();
        model.mission.active_runners = 1;
        model.mission.total_runners = 2;
        model.runners.summary = Some(crate::api::dashboards::runners::RunnersSummary {
            active_runners: 40,
            total_runners: 40,
            paused_runners: 0,
            draining_runners: 0,
            degraded_runners: 2,
            orphaned_containers: 1,
            partial_inventory: true,
        });

        let input = RunnersLensInput::from_read_model(&model);

        assert_eq!(input.active_runners, 40);
        assert_eq!(input.total_runners, 40);
        assert_eq!(input.degraded_runners, 2);
        assert_eq!(input.orphaned_containers, 1);
        assert!(input.partial_inventory);
    }
}
