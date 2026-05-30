//! Background engine tasks extracted from engine.rs to keep the main engine
//! module focused on webhook handling and startup wiring.

use super::{EngineState, SharedState};

#[path = "engine_background_events.rs"]
mod events;
#[path = "engine_background_health.rs"]
mod health;
#[path = "engine_background_metrics.rs"]
mod metrics;
#[path = "engine_background_reconcile.rs"]
mod reconcile;
#[path = "engine_background_runner_heal.rs"]
mod runner_heal;

pub(crate) use events::docker_event_loop;
pub(crate) use health::system_health_loop;
pub(crate) use metrics::cache_summary;
pub(crate) use reconcile::{check_scale_up, reconciliation_loop};
pub(crate) use runner_heal::runner_heal_loop;
