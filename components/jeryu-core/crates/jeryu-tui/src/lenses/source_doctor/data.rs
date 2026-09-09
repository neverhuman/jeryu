//! Owner: Interactive TUI subsystem - Source Doctor lens data selector
//! Proof: `cargo test -p jeryu-tui --lib lenses::source_doctor::data`
//! Invariants: Pure projection from `TuiReadModel` to
//!             `SourceDoctorLensInput`. No I/O. Render layer reads only the
//!             resulting struct. Component rows are projected from
//!             `model.system` (scm/database/sandbox/cache/vault via
//!             `SystemHealth::components()`) plus a synthetic runners row
//!             derived from `model.system.runners` (`RunnerHealth`).

use jeryu_readmodel::HealthLevel;
use jeryu_readmodel::TuiReadModel;

/// One infra/config component row in the Source Doctor diagnostic table.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRow {
    pub name: String,
    pub health: HealthLevel,
    /// Human-readable detail: the component message plus latency, or a
    /// runner fleet summary. Always a concrete diagnostic string.
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SourceDoctorLensInput {
    /// All infra/config components diagnosed this paint, in display order.
    pub components: Vec<ComponentRow>,
    pub event_cursor: u64,
}

impl SourceDoctorLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let system = &model.system;

        let mut components: Vec<ComponentRow> = system
            .components()
            .iter()
            .map(|c| {
                let mut detail = c.detail.clone().unwrap_or_default();
                if let Some(ms) = c.latency_ms {
                    if detail.is_empty() {
                        detail = format!("{ms}ms");
                    } else {
                        detail = format!("{detail} ({ms}ms)");
                    }
                }
                if detail.is_empty() {
                    detail = "ok".to_string();
                }
                ComponentRow {
                    name: c.name.clone(),
                    health: c.status,
                    detail,
                }
            })
            .collect();

        // Runners are tracked as a fleet count rather than a ComponentHealth,
        // so project them into a row of their own.
        let r = &system.runners;
        let runner_health = if r.online == 0 {
            HealthLevel::Unknown
        } else if r.degraded > 0 {
            HealthLevel::Degraded
        } else {
            HealthLevel::Healthy
        };
        components.push(ComponentRow {
            name: "runners".to_string(),
            health: runner_health,
            detail: format!(
                "{} online · {} busy · {} idle · {} degraded",
                r.online, r.busy, r.idle, r.degraded
            ),
        });

        Self {
            components,
            event_cursor: model.event_cursor,
        }
    }

    /// Count of components reporting `Healthy`.
    pub fn healthy_count(&self) -> usize {
        self.components
            .iter()
            .filter(|c| c.health == HealthLevel::Healthy)
            .count()
    }

    /// Total components diagnosed.
    pub fn total_count(&self) -> usize {
        self.components.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_returns_unknown_components() {
        let model = TuiReadModel::default();
        let input = SourceDoctorLensInput::from_read_model(&model);
        // 5 infra components + 1 runners row.
        assert_eq!(input.total_count(), 6);
        let names: Vec<&str> = input.components.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"scm"));
        assert!(names.contains(&"database"));
        assert!(names.contains(&"runners"));
        // Default infra components are "not yet checked" (Degraded), and the
        // default runner fleet is empty (Unknown) — none are Healthy.
        assert_eq!(input.healthy_count(), 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = SourceDoctorLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn runner_row_reflects_fleet_counts() {
        let mut model = TuiReadModel::default();
        model.system.runners.online = 4;
        model.system.runners.busy = 2;
        model.system.runners.idle = 2;
        model.system.runners.degraded = 0;
        let input = SourceDoctorLensInput::from_read_model(&model);
        let runners = input
            .components
            .iter()
            .find(|c| c.name == "runners")
            .expect("runners row present");
        assert_eq!(runners.health, HealthLevel::Healthy);
        assert!(runners.detail.contains("4 online"));
    }

    #[test]
    fn degraded_runner_fleet_is_degraded() {
        let mut model = TuiReadModel::default();
        model.system.runners.online = 3;
        model.system.runners.degraded = 1;
        let input = SourceDoctorLensInput::from_read_model(&model);
        let runners = input
            .components
            .iter()
            .find(|c| c.name == "runners")
            .unwrap();
        assert_eq!(runners.health, HealthLevel::Degraded);
    }
}
