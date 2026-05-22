use super::SharedState;

#[path = "engine_background_health_prelude.rs"]
mod prelude;
#[path = "engine_background_health_pressure.rs"]
mod pressure;
#[path = "engine_background_health_pressure_events.rs"]
mod pressure_events;
#[path = "engine_background_health_support.rs"]
mod support;

use self::prelude::{health_housekeeping, maybe_settle_startup_delay};
use self::support::health_cycle;

pub(crate) async fn system_health_loop(state: SharedState) {
    let mut auto_paused_pools = std::collections::BTreeSet::new();
    let mut consecutive_zero_freed = 0u32;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));

    maybe_settle_startup_delay().await;

    loop {
        interval.tick().await;
        health_housekeeping(&state).await;
        health_cycle(&state, &mut auto_paused_pools, &mut consecutive_zero_freed).await;
    }
}
