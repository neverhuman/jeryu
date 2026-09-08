//! Owner: Interactive TUI subsystem - Cache lens data selector
//! Proof: `cargo test -p jeryu-tui --lib lenses::cache::data`
//! Invariants: Pure projection from `TuiReadModel` to `CacheLensInput`.
//!             No I/O. Reflects SmartCache health: hit ratio, taints, and the
//!             `system.cache` component health.

use jeryu_readmodel::HealthLevel;
use jeryu_readmodel::TuiReadModel;

#[derive(Debug, Clone)]
pub struct CacheLensInput {
    /// SmartCache hit ratio in `0.0..=1.0`.
    pub cache_hit_ratio: f64,
    /// Currently-active cache taints (live invalidations).
    pub active_taints: u32,
    /// Total taints recorded (historical), from `mission.taint_count`.
    pub taint_count: u32,
    /// Health level of the `system.cache` component.
    pub component_status: HealthLevel,
    /// Component display name (e.g. `cache`).
    pub component_name: String,
    /// Optional human-readable detail / failure reason.
    pub component_detail: Option<String>,
    /// Optional last-probe latency for the cache component.
    pub component_latency_ms: Option<u64>,
    pub event_cursor: u64,
}

impl CacheLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let cache = &model.system.cache;
        Self {
            cache_hit_ratio: model.mission.cache_hit_ratio,
            active_taints: model.mission.active_taints,
            taint_count: model.mission.taint_count,
            component_status: cache.status,
            component_name: cache.name.clone(),
            component_detail: cache.detail.clone(),
            component_latency_ms: cache.latency_ms,
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
        assert_eq!(input.taint_count, 0);
        // Default `system.cache` is `unknown()` => Degraded with a detail.
        assert_eq!(input.component_name, "cache");
        assert_eq!(input.component_status, HealthLevel::Degraded);
        assert!(input.component_detail.is_some());
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = CacheLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }
}
