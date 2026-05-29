//! Owner: Interactive TUI subsystem - Queue lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::queue::data`
//! Invariants: Pure projection from `TuiReadModel` to `QueueLensInput`.
//!             No I/O. Render layer reads only the resulting struct.
//!             Capacity math (headroom, utilization) is derived here so the
//!             view stays a dumb renderer.

use crate::api::read_model::{RunnerHealth, TuiReadModel};

#[derive(Debug, Clone)]
pub struct QueueLensInput {
    pub queue_depth: u32,
    pub running_jobs: u32,
    pub failed_jobs: u32,
    pub active_runners: u32,
    pub total_runners: u32,
    /// Per-runner health rollup from `system.runners` (online/busy/idle/degraded).
    pub runner_health: RunnerHealth,
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
            runner_health: model.system.runners.clone(),
            event_cursor: model.event_cursor,
        }
    }

    /// Free runner slots (theoretical headroom). Saturates at zero so an
    /// over-committed fleet never underflows.
    pub fn headroom(&self) -> u32 {
        self.total_runners.saturating_sub(self.active_runners)
    }

    /// Fleet utilization as a whole-percent (0 when no runners are known).
    pub fn utilization_pct(&self) -> u32 {
        if self.total_runners == 0 {
            0
        } else {
            (self.active_runners as f64 / self.total_runners as f64 * 100.0).round() as u32
        }
    }

    /// True when the queue cannot be absorbed by the currently free runners —
    /// jobs will wait. Drives the headroom warning colouring in the view.
    pub fn queue_exceeds_headroom(&self) -> bool {
        self.queue_depth > self.headroom()
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
        assert_eq!(input.event_cursor, 0);
        assert_eq!(input.headroom(), 0);
        assert_eq!(input.utilization_pct(), 0);
        assert!(!input.queue_exceeds_headroom());
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = QueueLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
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

    #[test]
    fn select_projects_runner_health() {
        let mut model = TuiReadModel::default();
        model.system.runners = RunnerHealth {
            online: 8,
            busy: 4,
            idle: 4,
            degraded: 1,
        };
        let input = QueueLensInput::from_read_model(&model);
        assert_eq!(input.runner_health.online, 8);
        assert_eq!(input.runner_health.degraded, 1);
    }

    #[test]
    fn capacity_math_derives_headroom_and_utilization() {
        let mut model = TuiReadModel::default();
        model.mission.active_runners = 3;
        model.mission.total_runners = 8;
        model.mission.queued_jobs = 2;
        let input = QueueLensInput::from_read_model(&model);
        assert_eq!(input.headroom(), 5);
        assert_eq!(input.utilization_pct(), 38);
        assert!(!input.queue_exceeds_headroom());
    }

    #[test]
    fn headroom_saturates_and_flags_overflow() {
        let mut model = TuiReadModel::default();
        model.mission.active_runners = 10;
        model.mission.total_runners = 8; // over-committed
        model.mission.queued_jobs = 5;
        let input = QueueLensInput::from_read_model(&model);
        assert_eq!(input.headroom(), 0);
        assert!(input.queue_exceeds_headroom());
    }
}
