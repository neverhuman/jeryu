//! Owner: Interactive TUI subsystem - Source Doctor lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::source_doctor::data`
//! Invariants: Pure projection from `TuiReadModel` to
//!             `SourceDoctorLensInput`. No I/O. Render layer reads only
//!             the resulting struct. Per-source freshness, schema drift,
//!             action drift, MCP drift, docs drift, and DB profile
//!             mismatch land in U29 proper. This first-cut stubs all
//!             counts to zero; U29 wires
//!             `crate::api::dashboards::source_doctor::SourceDoctorDashboard`.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct SourceDoctorLensInput {
    /// Total registered sources. Placeholder zero until U29 wires
    /// `SourceDoctorDashboard::summary::sources_total`.
    pub sources_total: u32,
    /// Healthy source count. Placeholder zero until U29 wires the
    /// dashboard projection.
    pub sources_healthy: u32,
    /// Degraded source count. Placeholder zero until U29 wires the
    /// dashboard projection.
    pub sources_degraded: u32,
    pub event_cursor: u64,
}

impl SourceDoctorLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            // placeholders; U29 proper populates from
            // SourceDoctorDashboard once that projection is wired into
            // TuiReadModel.
            sources_total: 0,
            sources_healthy: 0,
            sources_degraded: 0,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_returns_default_input() {
        let model = TuiReadModel::default();
        let input = SourceDoctorLensInput::from_read_model(&model);
        assert_eq!(input.sources_total, 0);
        assert_eq!(input.sources_healthy, 0);
        assert_eq!(input.sources_degraded, 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 4321,
            ..Default::default()
        };
        let input = SourceDoctorLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 4321);
    }
}
