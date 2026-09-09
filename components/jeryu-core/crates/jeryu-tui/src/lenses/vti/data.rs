//! Owner: Interactive TUI subsystem - VTI lens data selector
//! Proof: `cargo test -p jeryu-tui --lib lenses::vti::data`
//! Invariants: Pure projection from `TuiReadModel` to `VtiLensInput`.
//!             No I/O. Render layer reads only the resulting struct.

use jeryu_readmodel::HealthLevel;
use jeryu_readmodel::TuiReadModel;

/// Projection feeding the VTI (test-impact / test-selection) cockpit.
/// All fields are derived from `model.mission`; the lens never reads raw
/// DB/forge state.
#[derive(Debug, Clone)]
pub struct VtiLensInput {
    /// Selector misses observed in the last 24h — test-impact signal quality.
    pub selector_misses_24h: u32,
    /// Jobs currently executing.
    pub running_jobs: u32,
    /// Jobs that ended in failure — the primary alert signal.
    pub failed_jobs: u32,
    /// Jobs waiting for a runner.
    pub queued_jobs: u32,
    /// Overall mission posture, colored in the view.
    pub overall: HealthLevel,
    pub event_cursor: u64,
}

impl VtiLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            selector_misses_24h: model.mission.selector_misses_24h,
            running_jobs: model.mission.running_jobs,
            failed_jobs: model.mission.failed_jobs,
            queued_jobs: model.mission.queued_jobs,
            overall: model.mission.overall,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_zero() {
        let input = VtiLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.selector_misses_24h, 0);
        assert_eq!(input.running_jobs, 0);
        assert_eq!(input.failed_jobs, 0);
        assert_eq!(input.queued_jobs, 0);
        assert_eq!(input.overall, HealthLevel::Healthy);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_projects_mission_job_counts() {
        let mut model = TuiReadModel::default();
        model.mission.selector_misses_24h = 9;
        model.mission.running_jobs = 4;
        model.mission.failed_jobs = 2;
        model.mission.queued_jobs = 7;
        model.mission.overall = HealthLevel::Degraded;
        let input = VtiLensInput::from_read_model(&model);
        assert_eq!(input.selector_misses_24h, 9);
        assert_eq!(input.running_jobs, 4);
        assert_eq!(input.failed_jobs, 2);
        assert_eq!(input.queued_jobs, 7);
        assert_eq!(input.overall, HealthLevel::Degraded);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = VtiLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }
}
