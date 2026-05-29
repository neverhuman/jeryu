//! Owner: Interactive TUI subsystem - Source Doctor lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::source_doctor::data`
//! Invariants: Pure projection from `TuiReadModel` to
//!             `SourceDoctorLensInput`. No I/O. Render layer reads only
//!             the resulting struct. Per-source freshness, schema drift,
//!             action drift, MCP drift, docs drift, and DB profile
//!             mismatch come from `SourceDoctorDashboard` on the shared
//!             read model.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct SourceDoctorLensInput {
    pub sources_total: u32,
    pub sources_healthy: u32,
    pub sources_degraded: u32,
    pub source_down_count: u32,
    pub partial_sources: u32,
    pub event_cursor: u64,
}

impl SourceDoctorLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.source_doctor.summary.as_ref();
        Self {
            sources_total: summary.map(|s| s.sources_total).unwrap_or(0),
            sources_healthy: summary.map(|s| s.sources_healthy).unwrap_or(0),
            sources_degraded: summary.map(|s| s.sources_degraded).unwrap_or(0),
            source_down_count: summary.map(|s| s.source_down_count).unwrap_or(0),
            partial_sources: summary.map(|s| s.partial_sources).unwrap_or(0),
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
        assert_eq!(input.source_down_count, 0);
        assert_eq!(input.partial_sources, 0);
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

    #[test]
    fn select_uses_source_doctor_summary_from_read_model() {
        let mut model = TuiReadModel::default();
        model.source_doctor.summary =
            Some(crate::api::dashboards::source_doctor::SourceDoctorSummary {
                sources_total: 6,
                sources_healthy: 5,
                sources_degraded: 1,
                schema_drift_count: 0,
                source_down_count: 1,
                partial_sources: 2,
            });

        let input = SourceDoctorLensInput::from_read_model(&model);

        assert_eq!(input.sources_total, 6);
        assert_eq!(input.sources_healthy, 5);
        assert_eq!(input.sources_degraded, 1);
        assert_eq!(input.source_down_count, 1);
        assert_eq!(input.partial_sources, 2);
    }
}
