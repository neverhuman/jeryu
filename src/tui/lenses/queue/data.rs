//! Owner: Interactive TUI subsystem - Queue lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::queue::data`
//! Invariants: Pure projection from `TuiReadModel` to `QueueLensInput`.
//!             No I/O. Render layer reads only the resulting struct.
//!             Physics-floor / fleet / policy capacity math lands in U17
//!             proper — this first-cut only exposes raw counts.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct QueueLensInput {
    pub queue_depth: u32,
    pub running_jobs: u32,
    pub failed_jobs: u32,
    pub active_runners: u32,
    pub total_runners: u32,
    pub degraded_runners: u32,
    pub event_cursor: u64,
}

impl QueueLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            queue_depth: model.mission.queued_jobs,
            running_jobs: model.mission.running_jobs,
            failed_jobs: model.mission.failed_jobs,
            active_runners: model.mission.active_runners,
            total_runners: model.mission.total_runners,
            degraded_runners: model.system.runners.degraded,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_returns_zero_counts() {
        let model = TuiReadModel::default();
        let input = QueueLensInput::from_read_model(&model);
        assert_eq!(input.queue_depth, 0);
        assert_eq!(input.running_jobs, 0);
        assert_eq!(input.failed_jobs, 0);
        assert_eq!(input.active_runners, 0);
        assert_eq!(input.total_runners, 0);
        assert_eq!(input.degraded_runners, 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 5678,
            ..Default::default()
        };
        let input = QueueLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 5678);
    }

    #[test]
    fn select_maps_mission_queue_counts() {
        let mut model = TuiReadModel::default();
        model.mission.queued_jobs = 42;
        model.mission.running_jobs = 7;
        model.mission.failed_jobs = 3;
        model.mission.active_runners = 4;
        model.mission.total_runners = 8;
        let input = QueueLensInput::from_read_model(&model);
        assert_eq!(input.queue_depth, 42);
        assert_eq!(input.running_jobs, 7);
        assert_eq!(input.failed_jobs, 3);
        assert_eq!(input.active_runners, 4);
        assert_eq!(input.total_runners, 8);
    }
}
