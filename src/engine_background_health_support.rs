use std::collections::BTreeSet;

use tracing::warn;

use super::SharedState;
use super::pressure::{handle_nominal_pressure, handle_pressure_cycle};

pub(crate) async fn health_cycle(
    state: &SharedState,
    auto_paused_pools: &mut BTreeSet<String>,
    consecutive_zero_freed: &mut u32,
) {
    match crate::cache::df_usage("/").await {
        Ok(fs) => {
            let pressure = crate::cache::root_disk_pressure_level(fs.available_bytes);
            let root_free = fs.available_bytes;
            let root_used = fs.used_percent;

            if pressure == crate::cache::DiskPressureLevel::Nominal {
                handle_nominal_pressure(
                    state,
                    auto_paused_pools,
                    consecutive_zero_freed,
                    root_free,
                    root_used,
                )
                .await;
                return;
            }
            handle_pressure_cycle(
                state,
                auto_paused_pools,
                consecutive_zero_freed,
                pressure,
                root_free,
            )
            .await;
        }
        Err(e) => {
            warn!(error = %e, "failed to check disk usage");
        }
    }
}
