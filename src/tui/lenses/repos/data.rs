//! Owner: Interactive TUI subsystem - Repos lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::repos::data`
//! Invariants: Pure projection from `TuiReadModel` to `ReposLensInput`.
//!             No I/O. Render layer reads only the resulting struct.
//!             Real family/repo enumeration lands in U18 proper;
//!             this first-cut exposes only an aggregate count placeholder.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct ReposLensInput {
    /// Placeholder count. U18 proper pulls per-family / per-repo
    /// entries from `read_model.repos` once that surface is contracted.
    pub total_repos: u32,
    pub event_cursor: u64,
}

impl ReposLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            total_repos: 0, // placeholder; U18 proper populates from model.repos
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_returns_zero_repos() {
        let model = TuiReadModel::default();
        let input = ReposLensInput::from_read_model(&model);
        assert_eq!(input.total_repos, 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let mut model = TuiReadModel::default();
        model.event_cursor = 9012;
        let input = ReposLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 9012);
    }
}
