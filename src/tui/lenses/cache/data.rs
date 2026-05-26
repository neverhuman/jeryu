//! Owner: Interactive TUI subsystem - Cache lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::cache::data`
//! Invariants: Pure projection from `TuiReadModel` to `CacheLensInput`.
//!             No I/O.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct CacheLensInput {
    pub cache_hit_ratio: f64,
    pub active_taints: u32,
    pub event_cursor: u64,
}

impl CacheLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            cache_hit_ratio: model.mission.cache_hit_ratio,
            active_taints: model.mission.active_taints,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_zero() {
        let input = CacheLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.cache_hit_ratio, 0.0);
        assert_eq!(input.active_taints, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let mut model = TuiReadModel::default();
        model.event_cursor = 42;
        let input = CacheLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }
}
